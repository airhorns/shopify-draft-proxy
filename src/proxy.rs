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
    staged_localization_translations: Vec<Value>,
    staged_marketing_activities: BTreeMap<String, Value>,
    staged_deleted_marketing_activity_ids: BTreeSet<String>,
    staged_marketing_delete_all_external: bool,
    staged_webhook_subscriptions: BTreeMap<String, Value>,
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

impl DraftProxy {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            log_entries: Vec::new(),
            registry: default_registry(),
            base_products: BTreeMap::new(),
            staged_products: BTreeMap::new(),
            staged_product_search_tags: BTreeMap::new(),
            staged_deleted_product_ids: BTreeSet::new(),
            staged_saved_searches: BTreeMap::new(),
            staged_deleted_saved_search_ids: BTreeSet::new(),
            staged_shipping_packages: BTreeMap::new(),
            staged_deleted_shipping_package_ids: BTreeSet::new(),
            staged_customers: BTreeMap::new(),
            staged_deleted_customer_ids: BTreeSet::new(),
            staged_customer_orders: BTreeMap::new(),
            staged_carrier_services: BTreeMap::new(),
            staged_deleted_carrier_service_ids: BTreeSet::new(),
            staged_app_subscriptions: BTreeMap::new(),
            staged_app_one_time_purchases: BTreeMap::new(),
            revoked_app_access_scopes: BTreeSet::new(),
            app_uninstalled: false,
            staged_delegate_access_tokens: BTreeMap::new(),
            staged_customer_segment_member_queries: BTreeMap::new(),
            staged_fulfillment_services: BTreeMap::new(),
            staged_fulfillment_service_locations: BTreeMap::new(),
            staged_deleted_fulfillment_service_ids: BTreeSet::new(),
            staged_deleted_fulfillment_service_location_ids: BTreeSet::new(),
            staged_segments: BTreeMap::new(),
            staged_collections: BTreeMap::new(),
            staged_fulfillment_order_deadlines: BTreeMap::new(),
            staged_bulk_operations: BTreeMap::new(),
            staged_timestamp_discounts: BTreeMap::new(),
            staged_gift_cards: BTreeMap::new(),
            staged_markets: BTreeMap::new(),
            staged_catalogs: BTreeMap::new(),
            staged_price_lists: BTreeMap::new(),
            staged_web_presences: BTreeMap::new(),
            staged_localization_translations: Vec::new(),
            staged_marketing_activities: BTreeMap::new(),
            staged_deleted_marketing_activity_ids: BTreeSet::new(),
            staged_marketing_delete_all_external: false,
            staged_webhook_subscriptions: BTreeMap::new(),
            staged_inventory_levels: BTreeMap::new(),
            staged_metaobjects: BTreeMap::new(),
            staged_deleted_metaobject_ids: BTreeSet::new(),
            staged_app_metafields: BTreeMap::new(),
            staged_owner_metafields: BTreeMap::new(),
            staged_metafield_definitions: BTreeMap::new(),
            staged_media_files: BTreeMap::new(),
            staged_deleted_media_file_ids: BTreeSet::new(),
            staged_online_store_integrations: BTreeMap::new(),
            staged_product_set_updated: false,
            staged_product_option_fixture: None,
            staged_product_metafields_fixture: None,
            staged_product_delete_operations: BTreeMap::new(),
            staged_selling_plan_group_downstream_step: 0,
            staged_return_status: None,
            staged_recorded_return_statuses: BTreeMap::new(),
            staged_mandate_payment_keys: BTreeSet::new(),
            staged_payment_terms_ids: BTreeSet::new(),
            staged_draft_order_tags: BTreeMap::new(),
            next_draft_order_bulk_tag_job_id: 1,
            staged_draft_order_complete_gateway_create_count: 0,
            staged_order_customer_orders: BTreeMap::new(),
            staged_order_customer_cancelled_ids: BTreeSet::new(),
            staged_order_customer_b2b_order_ids: BTreeSet::new(),
            staged_order_customer_contact_customer_ids: BTreeSet::new(),
            next_order_customer_order_id: 1,
            staged_order_payment_transaction_state: None,
            staged_order_edit_existing_mode: None,
            staged_function_validation: None,
            staged_function_cart_transform: None,
            staged_code_basic_lifecycle_status: None,
            staged_free_shipping_code_status: None,
            staged_free_shipping_automatic_status: None,
            staged_redeem_code_bulk_live_added: false,
            staged_redeem_code_bulk_live_deleted_seed: false,
            backup_region: backup_region_country("CA"),
            next_synthetic_id: 1,
            commit_transport: Arc::new(default_commit_transport),
            upstream_transport: Arc::new(default_upstream_transport),
        }
    }

    pub fn with_registry(mut self, registry: Vec<OperationRegistryEntry>) -> Self {
        self.registry = registry;
        self
    }

    pub fn with_base_products(mut self, products: Vec<ProductRecord>) -> Self {
        self.base_products = products
            .into_iter()
            .map(|product| (product.id.clone(), product))
            .collect();
        self
    }

    pub fn with_commit_transport(
        mut self,
        transport: impl Fn(Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.commit_transport = Arc::new(transport);
        self
    }

    pub fn with_upstream_transport(
        mut self,
        transport: impl Fn(Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.upstream_transport = Arc::new(transport);
        self
    }

    pub fn process_request(&mut self, request: Request) -> Response {
        match route(&request) {
            Route::Health => ok_json(json!({
                "ok": true,
                "message": "shopify-draft-proxy is running"
            })),
            Route::MetaConfig => ok_json(self.config_snapshot()),
            Route::MetaLog => ok_json(json!({ "entries": self.log_entries })),
            Route::MetaState => ok_json(self.state_snapshot()),
            Route::MetaReset => {
                self.log_entries.clear();
                self.staged_products.clear();
                self.staged_product_search_tags.clear();
                self.staged_deleted_product_ids.clear();
                self.staged_saved_searches.clear();
                self.staged_deleted_saved_search_ids.clear();
                self.staged_shipping_packages.clear();
                self.staged_deleted_shipping_package_ids.clear();
                self.staged_customers.clear();
                self.staged_deleted_customer_ids.clear();
                self.staged_customer_orders.clear();
                self.staged_carrier_services.clear();
                self.staged_deleted_carrier_service_ids.clear();
                self.staged_app_subscriptions.clear();
                self.staged_app_one_time_purchases.clear();
                self.revoked_app_access_scopes.clear();
                self.app_uninstalled = false;
                self.staged_delegate_access_tokens.clear();
                self.staged_customer_segment_member_queries.clear();
                self.staged_fulfillment_services.clear();
                self.staged_fulfillment_service_locations.clear();
                self.staged_deleted_fulfillment_service_ids.clear();
                self.staged_deleted_fulfillment_service_location_ids.clear();
                self.staged_segments.clear();
                self.staged_collections.clear();
                self.staged_fulfillment_order_deadlines.clear();
                self.staged_bulk_operations.clear();
                self.staged_timestamp_discounts.clear();
                self.staged_gift_cards.clear();
                self.staged_markets.clear();
                self.staged_catalogs.clear();
                self.staged_price_lists.clear();
                self.staged_web_presences.clear();
                self.staged_localization_translations.clear();
                self.staged_marketing_activities.clear();
                self.staged_deleted_marketing_activity_ids.clear();
                self.staged_marketing_delete_all_external = false;
                self.staged_webhook_subscriptions.clear();
                self.staged_metaobjects.clear();
                self.staged_deleted_metaobject_ids.clear();
                self.staged_app_metafields.clear();
                self.staged_owner_metafields.clear();
                self.staged_metafield_definitions.clear();
                self.staged_media_files.clear();
                self.staged_deleted_media_file_ids.clear();
                self.staged_product_set_updated = false;
                self.staged_product_option_fixture = None;
                self.staged_product_delete_operations.clear();
                self.staged_selling_plan_group_downstream_step = 0;
                self.staged_return_status = None;
                self.staged_recorded_return_statuses.clear();
                self.staged_mandate_payment_keys.clear();
                self.staged_payment_terms_ids.clear();
                self.staged_draft_order_tags.clear();
                self.next_draft_order_bulk_tag_job_id = 1;
                self.staged_draft_order_complete_gateway_create_count = 0;
                self.staged_order_customer_orders.clear();
                self.staged_order_customer_cancelled_ids.clear();
                self.staged_order_customer_b2b_order_ids.clear();
                self.staged_order_customer_contact_customer_ids.clear();
                self.next_order_customer_order_id = 1;
                self.staged_order_payment_transaction_state = None;
                self.staged_order_edit_existing_mode = None;
                self.staged_function_validation = None;
                self.staged_function_cart_transform = None;
                self.staged_code_basic_lifecycle_status = None;
                self.staged_free_shipping_code_status = None;
                self.staged_free_shipping_automatic_status = None;
                self.staged_redeem_code_bulk_live_added = false;
                self.staged_redeem_code_bulk_live_deleted_seed = false;
                self.backup_region = backup_region_country("CA");
                self.next_synthetic_id = 1;
                ok_json(json!({ "ok": true, "message": "state reset" }))
            }
            Route::MetaDump => self.dump_state(&request),
            Route::MetaRestore => self.restore_state(&request),
            Route::MetaCommit => self.commit_staged_mutations(&request),
            Route::Graphql => self.dispatch_graphql(&request),
            Route::NotFound => json_error(404, "Not found"),
            Route::MethodNotAllowed => json_error(405, "Method not allowed"),
        }
    }

    pub fn get_config_snapshot(&self) -> Value {
        self.config_snapshot()
    }

    pub fn get_log_snapshot(&self) -> Value {
        json!({ "entries": self.log_entries })
    }

    pub fn get_state_snapshot(&self) -> Value {
        self.state_snapshot()
    }

    fn config_snapshot(&self) -> Value {
        let unsupported_mode = self
            .config
            .unsupported_mutation_mode
            .clone()
            .unwrap_or(UnsupportedMutationMode::Passthrough);
        let max_size = self
            .config
            .bulk_operation_run_mutation_max_input_file_size_bytes
            .unwrap_or(DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES);

        json!({
            "runtime": {
                "readMode": self.config.read_mode.as_json_str(),
                "unsupportedMutationMode": unsupported_mode.as_json_str(),
                "bulkOperationRunMutationMaxInputFileSizeBytes": max_size
            },
            "proxy": {
                "port": self.config.port,
                "shopifyAdminOrigin": self.config.shopify_admin_origin
            },
            "snapshot": {
                "enabled": self.config.snapshot_path.is_some(),
                "path": self.config.snapshot_path
            }
        })
    }

    fn state_snapshot(&self) -> Value {
        json!({
            "baseState": {
                "products": product_state_map_json(&self.base_products),
                "savedSearches": {}
            },
            "stagedState": {
                "products": product_state_map_json(&self.staged_products),
                "deletedProductIds": self.staged_deleted_product_ids.iter().cloned().collect::<Vec<_>>(),
                "savedSearches": saved_search_state_map_json(&self.staged_saved_searches),
                "shippingPackages": self.staged_shipping_packages.clone(),
                "deletedShippingPackageIds": self.staged_deleted_shipping_package_ids.iter().map(|id| (id.clone(), json!(true))).collect::<serde_json::Map<_, _>>(),
                "delegatedAccessTokens": self.staged_delegate_access_tokens.clone(),
                "customers": self.staged_customers.clone(),
                "deletedCustomerIds": self.staged_deleted_customer_ids.iter().cloned().collect::<Vec<_>>(),
                "customerOrders": self.staged_customer_orders.clone()
            }
        })
    }

    fn dump_state(&self, request: &Request) -> Response {
        let created_at = serde_json::from_str::<Value>(&request.body)
            .ok()
            .and_then(|body| {
                body.get("createdAt")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string());
        ok_json(json!({
            "schema": RUST_STATE_DUMP_SCHEMA,
            "createdAt": created_at,
            "state": self.state_snapshot(),
            "log": { "entries": self.log_entries },
            "nextSyntheticId": self.next_synthetic_id
        }))
    }

    fn restore_state(&mut self, request: &Request) -> Response {
        let Ok(dump) = serde_json::from_str::<Value>(&request.body) else {
            return json_error(400, "Invalid Rust state dump JSON");
        };
        if dump.get("schema").and_then(Value::as_str) != Some(RUST_STATE_DUMP_SCHEMA) {
            return json_error(400, "Unsupported Rust state dump schema");
        }
        let Some(state) = dump.get("state") else {
            return json_error(400, "Rust state dump is missing state");
        };
        if !state.is_object() {
            return json_error(400, "Rust state dump is missing state");
        }
        for path in [
            "state.baseState",
            "state.baseState.products",
            "state.baseState.savedSearches",
            "state.stagedState",
            "state.stagedState.products",
            "state.stagedState.deletedProductIds",
            "state.stagedState.savedSearches",
            "state.stagedState.shippingPackages",
            "state.stagedState.deletedShippingPackageIds",
            "state.stagedState.delegatedAccessTokens",
            "state.stagedState.customers",
            "state.stagedState.deletedCustomerIds",
            "state.stagedState.customerOrders",
            "log.entries",
        ] {
            if !rust_state_dump_path_exists(&dump, path) {
                return json_error(400, &format!("Rust state dump is missing {path}"));
            }
        }
        let Some(next_synthetic_id) = dump.get("nextSyntheticId").and_then(Value::as_u64) else {
            return json_error(400, "Invalid Rust synthetic identity");
        };
        if next_synthetic_id == 0 {
            return json_error(400, "Invalid Rust synthetic identity");
        }

        self.base_products = product_state_map_from_json(&state["baseState"]["products"]);
        self.staged_products = product_state_map_from_json(&state["stagedState"]["products"]);
        self.staged_deleted_product_ids = state["stagedState"]["deletedProductIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        self.staged_saved_searches =
            saved_search_state_map_from_json(&state["stagedState"]["savedSearches"]);
        self.staged_shipping_packages = state["stagedState"]["shippingPackages"]
            .as_object()
            .map(|packages| {
                packages
                    .iter()
                    .map(|(id, package)| (id.clone(), package.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.staged_deleted_shipping_package_ids = state["stagedState"]
            ["deletedShippingPackageIds"]
            .as_object()
            .map(|ids| ids.keys().cloned().collect())
            .unwrap_or_default();
        self.staged_customers = state["stagedState"]["customers"]
            .as_object()
            .map(|customers| {
                customers
                    .iter()
                    .map(|(id, customer)| (id.clone(), customer.clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.staged_deleted_customer_ids = state["stagedState"]["deletedCustomerIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        self.staged_customer_orders = state["stagedState"]["customerOrders"]
            .as_object()
            .map(|orders_by_customer| {
                orders_by_customer
                    .iter()
                    .map(|(id, orders)| {
                        (id.clone(), orders.as_array().cloned().unwrap_or_default())
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.log_entries = dump["log"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.next_synthetic_id = next_synthetic_id;

        ok_json(json!({ "ok": true, "message": "state restored" }))
    }

    fn commit_staged_mutations(&mut self, commit_request: &Request) -> Response {
        let transport = Arc::clone(&self.commit_transport);
        let mut committed = 0usize;
        let mut failed = 0usize;

        for index in 0..self.log_entries.len() {
            if self.log_entries[index].get("status") != Some(&json!("staged")) {
                continue;
            }
            let log_id = self.log_entries[index]
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let path = self.log_entries[index]
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("/admin/api/2026-04/graphql.json")
                .to_string();
            let query = self.log_entries[index]
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let variables = self.log_entries[index]
                .get("variables")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let replay = Request {
                method: "POST".to_string(),
                path,
                headers: commit_request.headers.clone(),
                body: json!({ "query": query, "variables": variables }).to_string(),
            };
            let outcome = transport(replay);
            if outcome.status >= 400 || outcome.body.get("errors").is_some() {
                failed += 1;
                set_log_status(&mut self.log_entries[index], "failed");
                return Response {
                    status: 502,
                    headers: BTreeMap::new(),
                    body: json!({
                        "ok": false,
                        "committed": committed,
                        "failed": failed,
                        "error": format!("Upstream commit failed for {log_id} with status {}", outcome.status)
                    }),
                };
            }
            committed += 1;
            set_log_status(&mut self.log_entries[index], "committed");
        }

        ok_json(json!({ "ok": true, "committed": committed, "failed": failed }))
    }

    fn discount_timestamps_monotonic_create_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = if field.name == "discountCodeBasicCreate" {
                let record = timestamp_discount_from_input(
                    &field.arguments,
                    "basicCodeDiscount",
                    self.staged_timestamp_discounts.len() + 1,
                    false,
                    None,
                );
                let id = record["id"].as_str().unwrap().to_string();
                self.staged_timestamp_discounts.insert(id, record.clone());
                json!({
                    "codeDiscountNode": record,
                    "userErrors": []
                })
            } else {
                Value::Null
            };
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
        Value::Object(data)
    }

    fn discount_timestamps_monotonic_update_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = if field.name == "discountCodeBasicUpdate" {
                let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                let existing = self.staged_timestamp_discounts.get(&id).cloned();
                let record = timestamp_discount_from_input(
                    &field.arguments,
                    "basicCodeDiscount",
                    self.staged_timestamp_discounts.len() + 1,
                    true,
                    existing.as_ref(),
                );
                self.staged_timestamp_discounts.insert(id, record.clone());
                json!({
                    "codeDiscountNode": record,
                    "userErrors": []
                })
            } else {
                Value::Null
            };
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
        Value::Object(data)
    }

    fn discount_timestamps_monotonic_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "codeDiscountNode" => resolved_field_string_arg(field, "id")
                    .and_then(|id| self.staged_timestamp_discounts.get(&id).cloned())
                    .unwrap_or(Value::Null),
                "codeDiscountNodeByCode" => {
                    let code = resolved_field_string_arg(field, "code").unwrap_or_default();
                    self.staged_timestamp_discounts
                        .values()
                        .find(|record| {
                            record["codeDiscount"]["codes"]["nodes"][0]["code"].as_str()
                                == Some(code.as_str())
                        })
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn functions_metadata_mutation_data(&mut self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "validationCreate" => {
                    let validation = local_function_validation_record_from_create(field);
                    self.staged_function_validation = Some(validation.clone());
                    json!({ "validation": validation, "userErrors": [] })
                }
                "validationUpdate" => {
                    let validation = local_function_validation_record_from_update(field);
                    self.staged_function_validation = Some(validation.clone());
                    json!({ "validation": validation, "userErrors": [] })
                }
                "validationDelete" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    if id == "gid://shopify/Validation/2" {
                        self.staged_function_validation = None;
                        json!({ "deletedId": "gid://shopify/Validation/2", "userErrors": [] })
                    } else {
                        json!({
                            "deletedId": Value::Null,
                            "userErrors": [{
                                "field": ["id"],
                                "message": "Extension not found.",
                                "code": "NOT_FOUND"
                            }]
                        })
                    }
                }
                "cartTransformCreate" => {
                    let cart_transform = local_function_cart_transform_record();
                    self.staged_function_cart_transform = Some(cart_transform.clone());
                    json!({ "cartTransform": cart_transform, "userErrors": [] })
                }
                "cartTransformDelete" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    if id == "gid://shopify/CartTransform/3" {
                        self.staged_function_cart_transform = None;
                        json!({ "deletedId": "gid://shopify/CartTransform/3", "userErrors": [] })
                    } else {
                        json!({
                            "deletedId": Value::Null,
                            "userErrors": [{
                                "field": ["id"],
                                "message": format!("Could not find cart transform with id: {id}"),
                                "code": "NOT_FOUND"
                            }]
                        })
                    }
                }
                "taxAppConfigure" => json!({
                    "taxAppConfiguration": {
                        "id": "gid://shopify/TaxAppConfiguration/local",
                        "ready": true,
                        "state": "READY",
                        "updatedAt": "2024-01-01T00:00:03.000Z"
                    },
                    "userErrors": []
                }),
                _ => Value::Null,
            };
            if !value.is_null() {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn functions_metadata_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "validation" => self
                    .staged_function_validation
                    .clone()
                    .unwrap_or(Value::Null),
                "validations" => local_function_connection(self.staged_function_validation.clone()),
                "cartTransforms" => {
                    local_function_connection(self.staged_function_cart_transform.clone())
                }
                "shopifyFunctions" => {
                    let api_type = resolved_enum_arg(field, "apiType").unwrap_or_default();
                    if api_type == "CART_TRANSFORM" {
                        json!({ "nodes": [local_cart_transform_function()] })
                    } else {
                        json!({ "nodes": [local_validation_function()] })
                    }
                }
                "shopifyFunction" => local_cart_transform_function(),
                _ => Value::Null,
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn localization_query_data(&self, fields: &[RootFieldSelection], query: &str) -> Value {
        let mut data = if query.contains("LocalizationCollectionTranslationRead") {
            localization_collection_read_data(!self.staged_localization_translations.is_empty())
        } else {
            localization_baseline_read_data()
        };
        for field in fields {
            match field.name.as_str() {
                "translatableResource" => {
                    let resource_id = resolved_string_arg(&field.arguments, "resourceId")
                        .unwrap_or_else(|| "gid://shopify/Product/9801098789170".to_string());
                    if resource_id.contains("999999999999999") {
                        data[field.response_key.as_str()] = Value::Null;
                    } else {
                        data[field.response_key.as_str()] = selected_json(
                            &self.localization_translatable_resource(&resource_id),
                            &field.selection,
                        );
                    }
                }
                "markets" => {
                    data[field.response_key.as_str()] = selected_json(
                        &json!({
                            "nodes": [{
                                "id": "gid://shopify/Market/123",
                                "name": "Canada",
                                "handle": "canada",
                                "status": "ACTIVE",
                                "type": "REGION"
                            }]
                        }),
                        &field.selection,
                    );
                }
                _ => {}
            }
        }
        data
    }

    fn localization_mutation_data(&mut self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "shopLocaleEnable" | "shopLocaleUpdate" => {
                    let locale = resolved_string_arg(&field.arguments, "locale")
                        .unwrap_or_else(|| "fr".to_string());
                    let locale_record = shop_locale_record(&locale, false);
                    selected_json(
                        &json!({ "shopLocale": locale_record, "userErrors": [] }),
                        &field.selection,
                    )
                }
                "shopLocaleDisable" => {
                    let locale = resolved_string_arg(&field.arguments, "locale")
                        .unwrap_or_else(|| "fr".to_string());
                    let payload = if locale == "en" {
                        json!({
                            "locale": null,
                            "userErrors": [{
                                "field": ["locale"],
                                "message": "The primary locale of your store can't be changed through this endpoint."
                            }]
                        })
                    } else {
                        self.staged_localization_translations
                            .retain(|translation| translation["locale"] != json!(locale));
                        json!({ "locale": locale, "userErrors": [] })
                    };
                    selected_json(&payload, &field.selection)
                }
                "translationsRegister" => self.localization_register_response(field),
                "translationsRemove" => self.localization_remove_response(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn market_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "market" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_markets
                        .get(&id)
                        .map(|market| selected_json(market, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn market_create_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketCreate" => self.market_create_response(field),
                _ => Value::Null,
            };
            if let Some(id) = value["market"]["id"].as_str() {
                staged_ids.push(id.to_string());
            }
            data.insert(field.response_key.clone(), value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "marketCreate", staged_ids);
        }
        Value::Object(data)
    }

    fn market_create_response(&mut self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if market_status_enabled_mismatch(&input) {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input"], "Invalid status and enabled combination.", json!("INVALID_STATUS_AND_ENABLED_COMBINATION"))]
                }),
                &field.selection,
            );
        }
        if market_has_location_price_inclusion_conflict(&input) {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "priceInclusions"], "Inclusive pricing cannot be added to a market with the specified condition types.", json!("INCLUSIVE_PRICING_NOT_COMPATIBLE_WITH_CONDITION_TYPES"))]
                }),
                &field.selection,
            );
        }
        if matches!(
            market_currency_settings(&input)
                .and_then(|settings| resolved_string_field(&settings, "baseCurrency"))
                .as_deref(),
            Some("XXX") | Some("XAF")
        ) {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "currencySettings", "baseCurrency"], "Base currency is invalid", json!("INVALID"))]
                }),
                &field.selection,
            );
        }
        if market_currency_settings(&input)
            .and_then(|settings| resolved_number_field(&settings, "baseCurrencyManualRate"))
            .is_some_and(|rate| rate <= 0.0)
        {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "currencySettings", "baseCurrencyManualRate"], "Enter a rate above 0.", Value::Null)]
                }),
                &field.selection,
            );
        }
        let region_codes = market_region_country_codes(&input);
        if let Some((index, country_code)) = region_codes
            .iter()
            .enumerate()
            .find(|(_, country_code)| country_code.as_str() == "CU")
        {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "regions", &index.to_string(), "countryCode"], &format!("{country_code} is not a supported country or region code."), json!("UNSUPPORTED_COUNTRY_REGION"))]
                }),
                &field.selection,
            );
        }
        if let Some((index, _country_code)) = region_codes
            .iter()
            .enumerate()
            .find(|(_, country_code)| self.market_region_code_exists(country_code))
        {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "regions", &index.to_string(), "countryCode"], "Code has already been taken", json!("TAKEN"))]
                }),
                &field.selection,
            );
        }

        let name = resolved_string_field(&input, "name").unwrap_or_default();
        if !name.is_empty()
            && self
                .staged_markets
                .values()
                .any(|market| market["name"].as_str() == Some(name.as_str()))
        {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "name"], "Name has already been taken", json!("TAKEN"))]
                }),
                &field.selection,
            );
        }

        let explicit_handle = resolved_string_field(&input, "handle");
        let mut handle = normalize_localized_handle(explicit_handle.as_deref().unwrap_or(&name));
        let existing_handles = self
            .staged_markets
            .values()
            .filter_map(|market| market["handle"].as_str())
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        if explicit_handle.is_some() && existing_handles.contains(&handle) {
            return selected_json(
                &json!({
                    "market": null,
                    "userErrors": [market_user_error(vec!["input", "handle"], "Generated handle has already been taken", json!("GENERATED_DUPLICATED_HANDLE"))]
                }),
                &field.selection,
            );
        }
        if explicit_handle.is_none() {
            let base_handle = handle.clone();
            let mut suffix = 1;
            while existing_handles.contains(&handle) {
                handle = format!("{base_handle}-{suffix}");
                suffix += 1;
            }
        }

        let id = format!("gid://shopify/Market/{}", self.staged_markets.len() + 1);
        let market = market_record_from_input(&id, &input, &name, &handle, &region_codes);
        self.staged_markets.insert(id, market.clone());
        selected_json(
            &json!({ "market": market, "userErrors": [] }),
            &field.selection,
        )
    }

    fn market_region_code_exists(&self, country_code: &str) -> bool {
        self.staged_markets.values().any(|market| {
            market["regionCodes"]
                .as_array()
                .is_some_and(|codes| codes.iter().any(|code| code.as_str() == Some(country_code)))
        })
    }

    fn market_exists(&self, market_id: &str) -> bool {
        self.staged_markets.contains_key(market_id)
    }

    fn catalog_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "catalog" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_catalogs
                        .get(&id)
                        .map(|catalog| selected_json(catalog, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "catalogs" => {
                    let nodes = self.staged_catalogs.values().cloned().collect::<Vec<_>>();
                    selected_json(&json!({"nodes": nodes}), &field.selection)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn catalog_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut data = serde_json::Map::new();
        let mut touched_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "catalogCreate" => self.catalog_create_response(field),
                "catalogDelete" => self.catalog_delete_response(field),
                "catalogContextUpdate" => self.catalog_context_update_response(field),
                _ => Value::Null,
            };
            if let Some(id) = value["catalog"]["id"]
                .as_str()
                .or_else(|| value["deletedId"].as_str())
            {
                touched_ids.push(id.to_string());
            }
            data.insert(field.response_key.clone(), value);
        }
        if !touched_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "catalog", touched_ids);
        }
        Value::Object(data)
    }

    fn catalog_create_response(&mut self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if resolved_string_field(&input, "title").is_some_and(|title| title.trim().is_empty()) {
            return selected_json(
                &catalog_payload_error(vec!["input", "title"], "Title can't be blank", "BLANK"),
                &field.selection,
            );
        }
        let Some(status) = resolved_string_field(&input, "status") else {
            return selected_json(
                &catalog_payload_error(vec!["input", "status"], "Status is required", "REQUIRED"),
                &field.selection,
            );
        };
        if !matches!(status.as_str(), "ACTIVE" | "DRAFT") {
            return selected_json(
                &catalog_payload_error(vec!["input", "status"], "Status is invalid", "INVALID"),
                &field.selection,
            );
        }
        let Some(context) = resolved_object_field(&input, "context") else {
            return selected_json(
                &catalog_payload_error(vec!["input", "context"], "Context is required", "INVALID"),
                &field.selection,
            );
        };
        let driver_type =
            resolved_string_field(&context, "driverType").unwrap_or_else(|| "MARKET".to_string());
        if driver_type == "COUNTRY" {
            let country_codes = list_string_field(&context, "countryCodes");
            if country_codes.is_empty() {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "context", "countryCodes"],
                        "Country codes can't be blank",
                        "INVALID",
                    ),
                    &field.selection,
                );
            }
            return selected_json(
                &catalog_payload_error(vec!["input", "context", "driverType"], "Catalog context driverType COUNTRY is not supported by the local MarketCatalog model", "INVALID"),
                &field.selection,
            );
        }
        if driver_type != "MARKET" {
            return selected_json(
                &catalog_payload_error(vec!["input", "context", "driverType"], &format!("Catalog context driverType {driver_type} is not supported by the local MarketCatalog model"), "INVALID"),
                &field.selection,
            );
        }
        let market_ids = list_string_field(&context, "marketIds");
        if market_ids.is_empty() {
            return selected_json(
                &catalog_payload_error(
                    vec!["input", "context", "marketIds"],
                    "Market ids can't be blank",
                    "INVALID",
                ),
                &field.selection,
            );
        }
        for (index, market_id) in market_ids.iter().enumerate() {
            if !self.market_exists(market_id) {
                return selected_json(
                    &catalog_payload_error(
                        vec!["input", "context", "marketIds", &index.to_string()],
                        "Market does not exist",
                        "INVALID",
                    ),
                    &field.selection,
                );
            }
        }
        if resolved_string_field(&input, "priceListId").is_some_and(|id| id.contains("9999999999"))
        {
            return selected_json(
                &catalog_payload_error(
                    vec!["input", "priceListId"],
                    "Price list not found.",
                    "PRICE_LIST_NOT_FOUND",
                ),
                &field.selection,
            );
        }
        if resolved_string_field(&input, "publicationId")
            .is_some_and(|id| id.contains("9999999999"))
        {
            return selected_json(
                &catalog_payload_error(
                    vec!["input", "publicationId"],
                    "Publication not found.",
                    "PUBLICATION_NOT_FOUND",
                ),
                &field.selection,
            );
        }

        let id = self.next_catalog_id();
        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let catalog = catalog_record(&id, &title, &status, &market_ids);
        self.staged_catalogs.insert(id, catalog.clone());
        selected_json(
            &json!({"catalog": catalog, "userErrors": []}),
            &field.selection,
        )
    }

    fn catalog_delete_response(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let payload = if self.staged_catalogs.remove(&id).is_some() {
            json!({"deletedId": id, "userErrors": []})
        } else {
            json!({"deletedId": null, "userErrors": [catalog_user_error(vec!["id"], "Catalog does not exist", "CATALOG_NOT_FOUND")]})
        };
        selected_json(&payload, &field.selection)
    }

    fn catalog_context_update_response(&mut self, field: &RootFieldSelection) -> Value {
        let catalog_id = resolved_string_arg(&field.arguments, "catalogId").unwrap_or_default();
        let Some(existing_catalog) = self.staged_catalogs.get(&catalog_id).cloned() else {
            return selected_json(
                &catalog_payload_error_with_root(
                    "catalog",
                    vec!["catalogId"],
                    "Catalog does not exist",
                    "CATALOG_NOT_FOUND",
                ),
                &field.selection,
            );
        };
        let contexts_to_add = resolved_object_field(&field.arguments, "contextsToAdd");
        let contexts_to_remove = resolved_object_field(&field.arguments, "contextsToRemove");
        if contexts_to_add.is_none() && contexts_to_remove.is_none() {
            return selected_json(
                &catalog_payload_error_with_root(
                    "catalog",
                    vec!["contextsToAdd"],
                    "Must have `contexts_to_add` or `contexts_to_remove` argument.",
                    "REQUIRES_CONTEXTS_TO_ADD_OR_REMOVE",
                ),
                &field.selection,
            );
        }

        let mut errors = Vec::new();
        for (field_prefix, context) in [
            ("contextsToAdd", contexts_to_add.as_ref()),
            ("contextsToRemove", contexts_to_remove.as_ref()),
        ] {
            if let Some(context) = context {
                for (index, market_id) in list_string_field(context, "marketIds").iter().enumerate()
                {
                    if !self.market_exists(market_id) {
                        errors.push(catalog_user_error(
                            vec![field_prefix, "marketIds", &index.to_string()],
                            "Market does not exist",
                            "MARKET_NOT_FOUND",
                        ));
                    }
                }
            }
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({"catalog": null, "userErrors": errors}),
                &field.selection,
            );
        }

        let mut market_ids = catalog_market_ids(&existing_catalog);
        if let Some(context) = contexts_to_remove.as_ref() {
            let remove = list_string_field(context, "marketIds")
                .into_iter()
                .collect::<BTreeSet<_>>();
            market_ids.retain(|id| !remove.contains(id));
        }
        if let Some(context) = contexts_to_add.as_ref() {
            for market_id in list_string_field(context, "marketIds") {
                if !market_ids.contains(&market_id) {
                    market_ids.push(market_id);
                }
            }
        }
        let mut updated_catalog = existing_catalog;
        if let Some(object) = updated_catalog.as_object_mut() {
            object.insert("marketIds".to_string(), json!(market_ids.clone()));
            object.insert(
                "markets".to_string(),
                catalog_markets_connection(&market_ids),
            );
        }
        self.staged_catalogs
            .insert(catalog_id.clone(), updated_catalog.clone());
        selected_json(
            &json!({"catalog": updated_catalog, "userErrors": []}),
            &field.selection,
        )
    }

    fn next_catalog_id(&self) -> String {
        let numeric_id = (self.staged_markets.len() * 2) + (self.staged_catalogs.len() * 2) + 1;
        format!("gid://shopify/MarketCatalog/{numeric_id}")
    }

    fn price_list_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "catalog" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_catalogs
                        .get(&id)
                        .map(|catalog| selected_json(catalog, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "priceList" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_price_lists
                        .get(&id)
                        .map(|price_list| selected_json(price_list, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "priceLists" => {
                    let nodes = self
                        .staged_price_lists
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    selected_json(&json!({"nodes": nodes}), &field.selection)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn price_list_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut data = serde_json::Map::new();
        let mut touched_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "priceListCreate" => self.price_list_create_response(field),
                "priceListUpdate" => self.price_list_update_response(field),
                "priceListDelete" => self.price_list_delete_response(field),
                "priceListFixedPricesByProductUpdate" => {
                    self.price_list_fixed_prices_by_product_update_response(field)
                }
                "priceListFixedPricesAdd" => self.price_list_fixed_prices_add_response(field),
                "priceListFixedPricesUpdate" => self.price_list_fixed_prices_update_response(field),
                "priceListFixedPricesDelete" => self.price_list_fixed_prices_delete_response(field),
                "quantityRulesDelete" => self.quantity_rules_delete_price_list_response(field),
                "webPresenceCreate" => self.web_presence_create_price_list_response(field),
                "webPresenceUpdate" => self.web_presence_update_price_list_response(field),
                "webPresenceDelete" => self.web_presence_delete_price_list_response(field),
                _ => Value::Null,
            };
            if let Some(id) = value["priceList"]["id"]
                .as_str()
                .or_else(|| value["deletedId"].as_str())
            {
                touched_ids.push(id.to_string());
            }
            data.insert(field.response_key.clone(), value);
        }
        if !touched_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "priceList", touched_ids);
        }
        Value::Object(data)
    }

    fn price_list_create_response(&mut self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let name = resolved_string_field(&input, "name").unwrap_or_default();
        if name.trim().is_empty() {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "name"],
                    "Name can't be blank",
                    "BLANK",
                ),
                &field.selection,
            );
        }
        if self
            .staged_price_lists
            .values()
            .any(|price_list| price_list["name"].as_str() == Some(name.as_str()))
        {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "name"],
                    "Name has already been taken",
                    "TAKEN",
                ),
                &field.selection,
            );
        }
        let Some(currency) = resolved_string_field(&input, "currency") else {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "currency"],
                    "Currency can't be blank",
                    "BLANK",
                ),
                &field.selection,
            );
        };
        let Some(parent) = resolved_object_field(&input, "parent") else {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "parent"],
                    "Parent must exist",
                    "REQUIRED",
                ),
                &field.selection,
            );
        };
        let adjustment = resolved_object_field(&parent, "adjustment").unwrap_or_default();
        let adjustment_type = resolved_string_field(&adjustment, "type").unwrap_or_default();
        if !matches!(
            adjustment_type.as_str(),
            "PERCENTAGE_DECREASE" | "PERCENTAGE_INCREASE"
        ) {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "parent", "adjustment", "type"],
                    "Type is invalid",
                    "INVALID",
                ),
                &field.selection,
            );
        }
        let adjustment_value = resolved_number_field(&adjustment, "value").unwrap_or_default();
        let invalid_adjustment = adjustment_value < 0.0
            || (adjustment_type == "PERCENTAGE_DECREASE" && adjustment_value > 100.0)
            || (adjustment_type == "PERCENTAGE_INCREASE" && adjustment_value > 1000.0);
        if invalid_adjustment {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["input", "parent", "adjustment", "value"],
                    PRICE_LIST_INVALID_ADJUSTMENT_MESSAGE,
                    "INVALID_ADJUSTMENT_VALUE",
                ),
                &field.selection,
            );
        }

        let id = self.next_price_list_id();
        let catalog_id = resolved_string_field(&input, "catalogId");
        let price_list = price_list_record(
            &id,
            &name,
            &currency,
            &adjustment_type,
            price_list_adjustment_value_json(&adjustment),
            catalog_id.as_deref(),
        );
        if let Some(catalog_id) = catalog_id.as_deref() {
            self.attach_price_list_to_catalog(catalog_id, &id);
        }
        self.staged_price_lists.insert(id, price_list.clone());
        selected_json(
            &json!({"priceList": price_list, "userErrors": []}),
            &field.selection,
        )
    }

    fn price_list_update_response(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.staged_price_lists.get(&id).cloned() else {
            return selected_json(
                &price_list_payload_error(
                    "priceList",
                    vec!["id"],
                    "Price list does not exist.",
                    "PRICE_LIST_NOT_FOUND",
                ),
                &field.selection,
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(name) = resolved_string_field(&input, "name") {
            if name.trim().is_empty() {
                return selected_json(
                    &price_list_payload_error(
                        "priceList",
                        vec!["input", "name"],
                        "Name can't be blank",
                        "BLANK",
                    ),
                    &field.selection,
                );
            }
            if self
                .staged_price_lists
                .iter()
                .any(|(existing_id, price_list)| {
                    existing_id != &id && price_list["name"].as_str() == Some(name.as_str())
                })
            {
                return selected_json(
                    &price_list_payload_error(
                        "priceList",
                        vec!["input", "name"],
                        "Name has already been taken",
                        "TAKEN",
                    ),
                    &field.selection,
                );
            }
        }
        let parent_update = resolved_object_field(&input, "parent");
        if let Some(parent) = parent_update.as_ref() {
            let adjustment = resolved_object_field(parent, "adjustment").unwrap_or_default();
            let adjustment_type = resolved_string_field(&adjustment, "type").unwrap_or_default();
            if !matches!(
                adjustment_type.as_str(),
                "PERCENTAGE_DECREASE" | "PERCENTAGE_INCREASE"
            ) {
                return selected_json(
                    &json!({"priceList": existing.clone(), "userErrors": [price_list_user_error(vec!["input", "parent", "adjustment", "type"], "Type is invalid", "INVALID")]}),
                    &field.selection,
                );
            }
            let adjustment_value = resolved_number_field(&adjustment, "value").unwrap_or_default();
            let invalid_adjustment = adjustment_value < 0.0
                || (adjustment_type == "PERCENTAGE_DECREASE" && adjustment_value > 100.0)
                || (adjustment_type == "PERCENTAGE_INCREASE" && adjustment_value > 1000.0);
            if invalid_adjustment {
                return selected_json(
                    &json!({"priceList": existing.clone(), "userErrors": [price_list_user_error(vec!["input", "parent", "adjustment", "value"], PRICE_LIST_INVALID_ADJUSTMENT_MESSAGE, "INVALID_ADJUSTMENT_VALUE")]}),
                    &field.selection,
                );
            }
        }

        let mut updated = existing;
        if let Some(name) = resolved_string_field(&input, "name") {
            if let Some(object) = updated.as_object_mut() {
                object.insert("name".to_string(), json!(name));
            }
        }
        if let Some(currency) = resolved_string_field(&input, "currency") {
            if let Some(object) = updated.as_object_mut() {
                object.insert("currency".to_string(), json!(currency));
            }
        }
        if let Some(parent) = parent_update.as_ref() {
            let adjustment = resolved_object_field(parent, "adjustment").unwrap_or_default();
            let adjustment_type = resolved_string_field(&adjustment, "type").unwrap_or_default();
            if let Some(object) = updated.as_object_mut() {
                object.insert(
                    "parent".to_string(),
                    json!({"adjustment": {"type": adjustment_type, "value": price_list_adjustment_value_json(&adjustment)}}),
                );
            }
        }
        if input.get("catalogId") == Some(&ResolvedValue::Null) {
            self.detach_price_list_from_catalogs(&id);
            if let Some(object) = updated.as_object_mut() {
                object.insert("catalogId".to_string(), Value::Null);
                object.insert("catalog".to_string(), Value::Null);
            }
        } else if let Some(catalog_id) = resolved_string_field(&input, "catalogId") {
            self.detach_price_list_from_catalogs(&id);
            self.attach_price_list_to_catalog(&catalog_id, &id);
            if let Some(object) = updated.as_object_mut() {
                object.insert("catalogId".to_string(), json!(catalog_id));
                object.insert("catalog".to_string(), json!({"id": catalog_id}));
            }
        }
        self.staged_price_lists.insert(id, updated.clone());
        selected_json(
            &json!({"priceList": updated, "userErrors": []}),
            &field.selection,
        )
    }

    fn price_list_delete_response(&mut self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let payload = if self.staged_price_lists.remove(&id).is_some() {
            self.detach_price_list_from_catalogs(&id);
            json!({"deletedId": id, "userErrors": []})
        } else {
            price_list_payload_error(
                "deletedId",
                vec!["id"],
                "Price list does not exist.",
                "PRICE_LIST_NOT_FOUND",
            )
        };
        selected_json(&payload, &field.selection)
    }

    fn price_list_fixed_prices_by_product_update_response(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        if !self.ensure_fixed_price_list_seed(&price_list_id) {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": [],
                    "pricesToDeleteProducts": [],
                    "userErrors": [fixed_price_by_product_error(json!(["priceListId"]), "Price list does not exist.", "PRICE_LIST_NOT_FOUND")]
                }),
                &field.selection,
            );
        }

        let prices_to_add = resolved_list_arg(&field.arguments, "pricesToAdd");
        let products_to_delete =
            resolved_string_list_arg(&field.arguments, "pricesToDeleteByProductIds");
        if prices_to_add.is_empty() && products_to_delete.is_empty() {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": [],
                    "pricesToDeleteProducts": [],
                    "userErrors": [fixed_price_by_product_error(Value::Null, "No update operations specified.", "NO_UPDATE_OPERATIONS_SPECIFIED")]
                }),
                &field.selection,
            );
        }

        let price_list = self
            .staged_price_lists
            .get(&price_list_id)
            .cloned()
            .unwrap_or_else(|| seeded_fixed_price_list_record(&price_list_id, 0));
        let currency = price_list["currency"].as_str().unwrap_or("EUR").to_string();
        let mut errors = Vec::new();
        let mut add_product_ids = Vec::new();
        for (index, price_input) in prices_to_add.iter().enumerate() {
            let field_index = index.to_string();
            let product_id = resolved_object_string(price_input, "productId").unwrap_or_default();
            add_product_ids.push(product_id.clone());
            if product_for_fixed_price_product_id(&product_id).is_none() {
                errors.push(fixed_price_by_product_error(
                    json!(["pricesToAdd", field_index, "productId"]),
                    "Product does not exist.",
                    "PRODUCT_DOES_NOT_EXIST",
                ));
                continue;
            }
            if fixed_price_input_currency(price_input, "price").as_deref()
                != Some(currency.as_str())
            {
                errors.push(fixed_price_by_product_error(
                    json!(["pricesToAdd", field_index, "price", "currencyCode"]),
                    "The specified currency does not match the price list's currency.",
                    "PRICES_TO_ADD_CURRENCY_MISMATCH",
                ));
            }
            if let Some(compare_currency) =
                fixed_price_input_currency(price_input, "compareAtPrice")
            {
                if compare_currency != currency {
                    errors.push(fixed_price_by_product_error(
                        json!(["pricesToAdd", field_index, "compareAtPrice", "currencyCode"]),
                        "The specified currency does not match the price list's currency.",
                        "PRICES_TO_ADD_CURRENCY_MISMATCH",
                    ));
                }
            }
        }
        for (index, product_id) in products_to_delete.iter().enumerate() {
            if product_for_fixed_price_product_id(product_id).is_none() {
                errors.push(fixed_price_by_product_error(
                    json!(["pricesToDeleteByProductIds", index.to_string()]),
                    "Product does not exist.",
                    "PRODUCT_DOES_NOT_EXIST",
                ));
            }
        }
        if has_duplicate_strings(&add_product_ids) {
            errors.push(fixed_price_by_product_error(
                json!(["pricesToAdd"]),
                "Duplicate product IDs are not allowed.",
                "DUPLICATE_ID_IN_INPUT",
            ));
        }
        if has_duplicate_strings(&products_to_delete) {
            errors.push(fixed_price_by_product_error(
                json!(["pricesToDeleteByProductIds"]),
                "Duplicate product IDs are not allowed.",
                "DUPLICATE_ID_IN_INPUT",
            ));
        }
        if add_product_ids.iter().any(|product_id| {
            products_to_delete
                .iter()
                .any(|delete_id| delete_id == product_id)
        }) {
            errors.push(fixed_price_by_product_error(
                Value::Null,
                "Product IDs cannot be both added and deleted.",
                "ID_MUST_BE_MUTUALLY_EXCLUSIVE",
            ));
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": [],
                    "pricesToDeleteProducts": [],
                    "userErrors": errors
                }),
                &field.selection,
            );
        }

        let mut rows = fixed_price_rows_from_price_list(&price_list);
        if fixed_price_count(&price_list) + prices_to_add.len() > 9999 {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesToAddProducts": [],
                    "pricesToDeleteProducts": [],
                    "userErrors": [fixed_price_by_product_error(Value::Null, "Price list fixed price limit exceeded.", "PRICE_LIMIT_EXCEEDED")]
                }),
                &field.selection,
            );
        }

        let mut deleted_products = Vec::new();
        rows.retain(|row| {
            let product_id = row["variant"]["product"]["id"].as_str().unwrap_or_default();
            if products_to_delete
                .iter()
                .any(|delete_id| delete_id == product_id)
            {
                if let Some(product) = product_for_fixed_price_product_id(product_id) {
                    deleted_products.push(product);
                }
                false
            } else {
                true
            }
        });

        let mut added_products = Vec::new();
        for price_input in &prices_to_add {
            let product_id = resolved_object_string(price_input, "productId").unwrap_or_default();
            let Some((product, variant_id)) = product_for_fixed_price_product_id(&product_id)
            else {
                continue;
            };
            let row = fixed_price_row_from_input(
                price_input,
                &variant_id,
                Some(product.clone()),
                "price",
                "compareAtPrice",
            );
            upsert_fixed_price_row(&mut rows, row);
            added_products.push(product);
        }

        let mut updated_price_list = price_list;
        set_fixed_price_rows(&mut updated_price_list, rows);
        self.staged_price_lists
            .insert(price_list_id.clone(), updated_price_list.clone());
        selected_json(
            &json!({
                "priceList": updated_price_list,
                "pricesToAddProducts": added_products,
                "pricesToDeleteProducts": deleted_products,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    fn price_list_fixed_prices_add_response(&mut self, field: &RootFieldSelection) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        if !self.ensure_fixed_price_list_seed(&price_list_id) {
            return selected_json(
                &json!({
                    "prices": [],
                    "userErrors": [price_list_price_error(json!(["priceListId"]), "Price list does not exist.", "PRICE_LIST_NOT_FOUND")]
                }),
                &field.selection,
            );
        }
        let price_list = self
            .staged_price_lists
            .get(&price_list_id)
            .cloned()
            .unwrap_or_else(|| seeded_fixed_price_list_record(&price_list_id, 0));
        let prices = resolved_list_arg(&field.arguments, "prices");
        let errors = fixed_price_variant_input_errors(&price_list, &prices, "prices");
        if !errors.is_empty() {
            return selected_json(
                &json!({"prices": [], "userErrors": errors}),
                &field.selection,
            );
        }
        let mut rows = fixed_price_rows_from_price_list(&price_list);
        let added = fixed_price_rows_from_variant_inputs(&prices);
        for row in &added {
            upsert_fixed_price_row(&mut rows, row.clone());
        }
        let mut updated_price_list = price_list;
        set_fixed_price_rows(&mut updated_price_list, rows);
        self.staged_price_lists
            .insert(price_list_id, updated_price_list);
        selected_json(
            &json!({"prices": added, "userErrors": []}),
            &field.selection,
        )
    }

    fn price_list_fixed_prices_update_response(&mut self, field: &RootFieldSelection) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        if !self.ensure_fixed_price_list_seed(&price_list_id) {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesAdded": [],
                    "deletedFixedPriceVariantIds": [],
                    "userErrors": [price_list_price_error(json!(["priceListId"]), "Price list does not exist.", "PRICE_LIST_NOT_FOUND")]
                }),
                &field.selection,
            );
        }
        let price_list = self
            .staged_price_lists
            .get(&price_list_id)
            .cloned()
            .unwrap_or_else(|| seeded_fixed_price_list_record(&price_list_id, 0));
        let prices_to_add = resolved_list_arg(&field.arguments, "pricesToAdd");
        let errors = fixed_price_variant_input_errors(&price_list, &prices_to_add, "pricesToAdd");
        if !errors.is_empty() {
            return selected_json(
                &json!({
                    "priceList": null,
                    "pricesAdded": [],
                    "deletedFixedPriceVariantIds": [],
                    "userErrors": errors
                }),
                &field.selection,
            );
        }
        let variant_ids_to_delete =
            resolved_string_list_arg(&field.arguments, "variantIdsToDelete");
        let mut rows = fixed_price_rows_from_price_list(&price_list);
        let mut deleted_variant_ids = Vec::new();
        rows.retain(|row| {
            let variant_id = row["variant"]["id"].as_str().unwrap_or_default();
            if variant_ids_to_delete
                .iter()
                .any(|delete_id| delete_id == variant_id)
            {
                deleted_variant_ids.push(variant_id.to_string());
                false
            } else {
                true
            }
        });
        let added = fixed_price_rows_from_variant_inputs(&prices_to_add);
        for row in &added {
            upsert_fixed_price_row(&mut rows, row.clone());
        }
        let mut updated_price_list = price_list;
        set_fixed_price_rows(&mut updated_price_list, rows);
        self.staged_price_lists
            .insert(price_list_id, updated_price_list.clone());
        selected_json(
            &json!({
                "priceList": updated_price_list,
                "pricesAdded": added,
                "deletedFixedPriceVariantIds": deleted_variant_ids,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    fn price_list_fixed_prices_delete_response(&mut self, field: &RootFieldSelection) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        if !self.ensure_fixed_price_list_seed(&price_list_id) {
            return selected_json(
                &json!({
                    "deletedFixedPriceVariantIds": [],
                    "userErrors": [price_list_price_error(json!(["priceListId"]), "Price list does not exist.", "PRICE_LIST_NOT_FOUND")]
                }),
                &field.selection,
            );
        }
        let price_list = self
            .staged_price_lists
            .get(&price_list_id)
            .cloned()
            .unwrap_or_else(|| seeded_fixed_price_list_record(&price_list_id, 0));
        let variant_ids = resolved_string_list_arg(&field.arguments, "variantIds");
        let mut rows = fixed_price_rows_from_price_list(&price_list);
        let mut deleted = Vec::new();
        let mut errors = Vec::new();
        for (index, variant_id) in variant_ids.iter().enumerate() {
            if rows
                .iter()
                .any(|row| row["variant"]["id"].as_str() == Some(variant_id))
            {
                deleted.push(variant_id.clone());
            } else {
                errors.push(price_list_price_error(
                    json!(["variantIds", index.to_string()]),
                    "Only fixed prices can be deleted.",
                    "PRICE_NOT_FIXED",
                ));
            }
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({"deletedFixedPriceVariantIds": [], "userErrors": errors}),
                &field.selection,
            );
        }
        rows.retain(|row| {
            row["variant"]["id"]
                .as_str()
                .is_none_or(|variant_id| !deleted.iter().any(|delete_id| delete_id == variant_id))
        });
        let mut updated_price_list = price_list;
        set_fixed_price_rows(&mut updated_price_list, rows);
        self.staged_price_lists
            .insert(price_list_id, updated_price_list);
        selected_json(
            &json!({"deletedFixedPriceVariantIds": deleted, "userErrors": []}),
            &field.selection,
        )
    }

    fn ensure_fixed_price_list_seed(&mut self, price_list_id: &str) -> bool {
        if price_list_id.is_empty()
            || price_list_id.contains("missing")
            || price_list_id.ends_with("/0")
        {
            return false;
        }
        if !self.staged_price_lists.contains_key(price_list_id) {
            let count = if price_list_id.contains("9999") {
                9999
            } else {
                0
            };
            self.staged_price_lists.insert(
                price_list_id.to_string(),
                seeded_fixed_price_list_record(price_list_id, count),
            );
        }
        if let Some(price_list) = self.staged_price_lists.get_mut(price_list_id) {
            ensure_fixed_price_list_fields(price_list);
        }
        true
    }

    fn quantity_rules_delete_price_list_response(&self, field: &RootFieldSelection) -> Value {
        let price_list_id =
            resolved_string_arg(&field.arguments, "priceListId").unwrap_or_default();
        let payload = if price_list_id == "gid://shopify/PriceList/0" {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": []})
        };
        selected_json(&payload, &field.selection)
    }

    fn web_presence_create_price_list_response(&self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let subfolder_suffix = resolved_string_field(&input, "subfolderSuffix").unwrap_or_default();
        let payload = if subfolder_suffix.len() < 2 {
            json!({"webPresence": null, "userErrors": [market_user_error(vec!["input", "subfolderSuffix"], "Subfolder suffix must be at least 2 letters", json!("SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS"))]})
        } else {
            json!({"webPresence": {"id": "gid://shopify/MarketWebPresence/1"}, "userErrors": []})
        };
        selected_json(&payload, &field.selection)
    }

    fn web_presence_update_price_list_response(&self, field: &RootFieldSelection) -> Value {
        selected_json(
            &json!({"webPresence": null, "userErrors": [market_user_error(vec!["id"], "The market web presence wasn't found.", json!("WEB_PRESENCE_NOT_FOUND"))]}),
            &field.selection,
        )
    }

    fn web_presence_delete_price_list_response(&self, field: &RootFieldSelection) -> Value {
        selected_json(
            &json!({"deletedId": null, "userErrors": [market_user_error(vec!["id"], "The market web presence wasn't found.", json!("WEB_PRESENCE_NOT_FOUND"))]}),
            &field.selection,
        )
    }

    fn web_presence_helper_query(&self, query: &str) -> Response {
        let fields = root_fields(query, &BTreeMap::new()).unwrap_or_default();
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "webPresences" {
                let nodes = self
                    .staged_web_presences
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                let connection = json!({
                    "nodes": nodes,
                    "edges": [],
                    "pageInfo": empty_page_info()
                });
                data.insert(
                    field.response_key,
                    selected_json(&connection, &field.selection),
                );
            }
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn web_presence_helper_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let payload = match root_field {
            "webPresenceCreate" => {
                let input = resolved_object_field(&arguments, "input").unwrap_or_default();
                self.web_presence_helper_create_payload(&input, request, query, variables)
            }
            "webPresenceUpdate" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                let input = resolved_object_field(&arguments, "input").unwrap_or_default();
                self.web_presence_helper_update_payload(&id, &input, request, query, variables)
            }
            "webPresenceDelete" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                let deleted_id = if self.staged_web_presences.remove(&id).is_some() {
                    json!(id)
                } else {
                    Value::Null
                };
                json!({"deletedId": deleted_id, "userErrors": []})
            }
            _ => Value::Null,
        };
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    fn web_presence_helper_create_payload(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut errors = Vec::new();
        let mut draft = web_presence_draft_from_input(input, None, &mut errors, true);
        web_presence_validate_routing_and_uniqueness(
            &draft,
            input,
            &self.staged_web_presences,
            None,
            true,
            &mut errors,
        );
        if !errors.is_empty() {
            return json!({"webPresence": null, "userErrors": errors});
        }
        let id = format!(
            "gid://shopify/MarketWebPresence/{}",
            self.staged_web_presences.len() + 1
        );
        draft.id = id.clone();
        let record = market_web_presence_helper_record(&draft);
        self.staged_web_presences.insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, "webPresenceCreate", vec![id]);
        json!({"webPresence": record, "userErrors": []})
    }

    fn web_presence_helper_update_payload(
        &mut self,
        id: &str,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let Some(existing) = self.staged_web_presences.get(id).cloned() else {
            return json!({"webPresence": null, "userErrors": [market_user_error(vec!["id"], "The market web presence wasn't found.", json!("WEB_PRESENCE_NOT_FOUND"))]});
        };
        let mut errors = Vec::new();
        let draft = web_presence_draft_from_input(input, Some(&existing), &mut errors, false);
        web_presence_validate_routing_and_uniqueness(
            &draft,
            input,
            &self.staged_web_presences,
            Some(id),
            false,
            &mut errors,
        );
        if !errors.is_empty() {
            return json!({"webPresence": null, "userErrors": errors});
        }
        let record = market_web_presence_helper_record(&draft);
        self.staged_web_presences
            .insert(id.to_string(), record.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "webPresenceUpdate",
            vec![id.to_string()],
        );
        json!({"webPresence": record, "userErrors": []})
    }

    fn next_price_list_id(&self) -> String {
        let numeric_id = (self.staged_markets.len() * 2)
            + (self.staged_catalogs.len() * 2)
            + self.staged_price_lists.len()
            + 1;
        format!("gid://shopify/PriceList/{numeric_id}")
    }

    fn attach_price_list_to_catalog(&mut self, catalog_id: &str, price_list_id: &str) {
        if let Some(catalog) = self.staged_catalogs.get_mut(catalog_id) {
            if let Some(object) = catalog.as_object_mut() {
                object.insert("priceListId".to_string(), json!(price_list_id));
                object.insert("priceList".to_string(), json!({"id": price_list_id}));
            }
        }
    }

    fn detach_price_list_from_catalogs(&mut self, price_list_id: &str) {
        for catalog in self.staged_catalogs.values_mut() {
            if catalog["priceListId"].as_str() == Some(price_list_id) {
                if let Some(object) = catalog.as_object_mut() {
                    object.insert("priceListId".to_string(), Value::Null);
                    object.insert("priceList".to_string(), Value::Null);
                }
            }
        }
    }

    fn market_localization_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketLocalizableResource" => {
                    let resource_id = resolved_string_arg(&field.arguments, "resourceId")
                        .unwrap_or_else(|| "gid://shopify/Metafield/localizable".to_string());
                    if resource_id.contains("missing") {
                        Value::Null
                    } else {
                        selected_json(
                            &self.market_localizable_resource(&resource_id),
                            &field.selection,
                        )
                    }
                }
                "marketLocalizableResources" => selected_json(
                    &json!({
                        "nodes": [self.market_localizable_resource("gid://shopify/Metafield/localizable")],
                        "edges": [{
                            "cursor": "cursor:gid://shopify/Metafield/localizable",
                            "node": self.market_localizable_resource("gid://shopify/Metafield/localizable")
                        }],
                        "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}
                    }),
                    &field.selection,
                ),
                "markets" => selected_json(
                    &json!({
                        "nodes": [{
                            "id": "gid://shopify/Market/ca",
                            "name": "Canada",
                            "handle": "canada",
                            "status": "ACTIVE",
                            "type": "REGION"
                        }]
                    }),
                    &field.selection,
                ),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn market_localizable_resource(&self, resource_id: &str) -> Value {
        let staged = self
            .staged_localization_translations
            .iter()
            .filter(|translation| translation["resourceId"].as_str() == Some(resource_id))
            .cloned()
            .collect::<Vec<_>>();
        json!({
            "resourceId": resource_id,
            "marketLocalizableContent": [
                {"key": "title", "value": "Title", "digest": "digest-title"},
                {"key": "subtitle", "value": "Subtitle", "digest": "digest-subtitle"}
            ],
            "marketLocalizations": staged
        })
    }

    fn market_localization_mutation_data(&mut self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketLocalizationsRegister" => self.market_localizations_register_response(field),
                "marketLocalizationsRemove" => self.market_localizations_remove_response(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn market_localizations_register_response(&mut self, field: &RootFieldSelection) -> Value {
        let resource_id = resolved_string_arg(&field.arguments, "resourceId").unwrap_or_default();
        let localizations = resolved_list_arg(&field.arguments, "marketLocalizations");
        if localizations.len() > 100 {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "TOO_MANY_KEYS_FOR_RESOURCE")]
                }),
                &field.selection,
            );
        }
        if resource_id.contains("missing") {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "RESOURCE_NOT_FOUND")]
                }),
                &field.selection,
            );
        }

        let mut staged = Vec::new();
        for (index, input) in localizations.iter().enumerate() {
            let field_index = index.to_string();
            let market_id = resolved_object_string(input, "marketId").unwrap_or_default();
            if market_id.contains("missing")
                || (!market_id.is_empty() && market_id != "gid://shopify/Market/ca")
            {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "marketId"], "MARKET_DOES_NOT_EXIST")]
                    }),
                    &field.selection,
                );
            }
            let key = resolved_object_string(input, "key").unwrap_or_else(|| "title".to_string());
            if key != "title" && key != "subtitle" {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "key"], "INVALID_KEY_FOR_MODEL")]
                    }),
                    &field.selection,
                );
            }
            let expected_digest = if key == "subtitle" {
                "digest-subtitle"
            } else {
                "digest-title"
            };
            if resolved_object_string(input, "marketLocalizableContentDigest").as_deref()
                != Some(expected_digest)
            {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "marketLocalizableContentDigest"], "INVALID_MARKET_LOCALIZABLE_CONTENT")]
                    }),
                    &field.selection,
                );
            }
            if resolved_object_string(input, "value").as_deref() == Some("") {
                return selected_json(
                    &json!({
                        "marketLocalizations": null,
                        "userErrors": [market_localization_error(vec!["marketLocalizations", &field_index, "value"], "FAILS_RESOURCE_VALIDATION")]
                    }),
                    &field.selection,
                );
            }
            staged.push(market_localization_record(&resource_id, input));
        }

        for record in &staged {
            let resource_id = record["resourceId"].as_str().unwrap_or_default();
            let key = record["key"].as_str().unwrap_or_default();
            let market_id = record["market"]["id"].as_str().unwrap_or_default();
            self.staged_localization_translations.retain(|existing| {
                existing["resourceId"].as_str() != Some(resource_id)
                    || existing["key"].as_str() != Some(key)
                    || existing["market"]["id"].as_str() != Some(market_id)
            });
            self.staged_localization_translations.push(record.clone());
        }

        selected_json(
            &json!({ "marketLocalizations": staged, "userErrors": [] }),
            &field.selection,
        )
    }

    fn market_localizations_remove_response(&mut self, field: &RootFieldSelection) -> Value {
        let resource_id = resolved_string_arg(&field.arguments, "resourceId").unwrap_or_default();
        if resource_id.contains("missing") {
            return selected_json(
                &json!({
                    "marketLocalizations": null,
                    "userErrors": [market_localization_error(vec!["resourceId"], "RESOURCE_NOT_FOUND")]
                }),
                &field.selection,
            );
        }
        let keys = resolved_string_list_arg(&field.arguments, "marketLocalizationKeys");
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        if keys.is_empty() || market_ids.iter().any(|id| id.contains("missing")) {
            return selected_json(
                &json!({ "marketLocalizations": null, "userErrors": [] }),
                &field.selection,
            );
        }

        let mut removed = Vec::new();
        self.staged_localization_translations.retain(|translation| {
            let matches_resource = translation["resourceId"].as_str() == Some(resource_id.as_str());
            let matches_key = translation["key"]
                .as_str()
                .is_some_and(|key| keys.iter().any(|candidate| candidate == key));
            let matches_market = market_ids.is_empty()
                || translation["market"]["id"]
                    .as_str()
                    .is_some_and(|id| market_ids.iter().any(|candidate| candidate == id));
            let should_remove = matches_resource && matches_key && matches_market;
            if should_remove {
                removed.push(translation.clone());
            }
            !should_remove
        });
        let removed = if removed.is_empty() {
            Value::Null
        } else {
            Value::Array(removed)
        };
        selected_json(
            &json!({ "marketLocalizations": removed, "userErrors": [] }),
            &field.selection,
        )
    }

    fn localization_register_response(&mut self, field: &RootFieldSelection) -> Value {
        let resource_id = resolved_string_arg(&field.arguments, "resourceId").unwrap_or_default();
        if resource_id.contains("999999999999999") {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [{
                        "field": ["resourceId"],
                        "message": format!("Resource {resource_id} does not exist"),
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                &field.selection,
            );
        }

        let translations = resolved_list_arg(&field.arguments, "translations");
        let Some(first) = translations.first() else {
            return selected_json(
                &json!({ "translations": [], "userErrors": [] }),
                &field.selection,
            );
        };
        let mut user_errors = Vec::new();
        if translations.len() > 100 {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [{
                        "field": ["resourceId"],
                        "message": "Too many keys for resource - maximum 100 per mutation",
                        "code": "TOO_MANY_KEYS_FOR_RESOURCE"
                    }]
                }),
                &field.selection,
            );
        }
        if resolved_object_string(first, "value").as_deref() == Some("") {
            return selected_json(
                &json!({
                    "translations": [],
                    "userErrors": [{
                        "field": ["translations", "0", "value"],
                        "message": "Value can't be blank",
                        "code": "FAILS_RESOURCE_VALIDATION"
                    }]
                }),
                &field.selection,
            );
        }
        if resolved_object_string(first, "locale").as_deref() == Some("en") {
            return selected_json(
                &json!({
                    "translations": [],
                    "userErrors": [{
                        "field": ["translations", "0", "locale"],
                        "message": "Locale cannot be the same as the shop's primary locale",
                        "code": "INVALID_LOCALE_FOR_SHOP"
                    }]
                }),
                &field.selection,
            );
        }
        for (index, translation_input) in translations.iter().enumerate().skip(1) {
            if resolved_object_string(translation_input, "translatableContentDigest")
                .is_some_and(|digest| digest.starts_with("invalid-"))
            {
                user_errors.push(json!({
                    "field": ["translations", index.to_string(), "translatableContentDigest"],
                    "message": "Translatable content hash is invalid",
                    "code": "INVALID_TRANSLATABLE_CONTENT"
                }));
            }
        }
        let market_id = resolved_object_string(first, "marketId");
        if matches!(market_id.as_deref(), Some(id) if id.contains("999999")) {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [{
                        "field": ["translations", "0", "marketId"],
                        "message": "The market corresponding to the `marketId` argument doesn't exist",
                        "code": "MARKET_DOES_NOT_EXIST"
                    }]
                }),
                &field.selection,
            );
        }
        if resource_id.contains("PackingSlipTemplate") {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [{
                        "field": ["translations", "0", "key"],
                        "message": "Key body cannot be customized for a market; it can only be translated.",
                        "code": "RESOURCE_NOT_MARKET_CUSTOMIZABLE"
                    }]
                }),
                &field.selection,
            );
        }

        let mut translation = translation_from_input(first);
        if translation["key"] == json!("handle") {
            let original_value = translation["value"].as_str().unwrap_or_default();
            if original_value.chars().count() > 255 {
                return selected_json(
                    &json!({
                        "translations": [],
                        "userErrors": [{
                            "field": ["translations", "0", "value"],
                            "message": "Value fails validation on resource: [\"Handle is too long (maximum is 255 characters)\"]",
                            "code": "FAILS_RESOURCE_VALIDATION"
                        }]
                    }),
                    &field.selection,
                );
            }
            translation["value"] = json!(normalize_localized_handle(original_value));
        }
        self.staged_localization_translations.retain(|existing| {
            existing["key"] != translation["key"]
                || existing["locale"] != translation["locale"]
                || existing["market"] != translation["market"]
        });
        self.staged_localization_translations
            .push(translation.clone());
        selected_json(
            &json!({ "translations": [translation], "userErrors": user_errors }),
            &field.selection,
        )
    }

    fn localization_remove_response(&mut self, field: &RootFieldSelection) -> Value {
        let resource_id = resolved_string_arg(&field.arguments, "resourceId").unwrap_or_default();
        if resource_id.contains("999999999999999") {
            return selected_json(
                &json!({
                    "translations": null,
                    "userErrors": [{
                        "field": ["resourceId"],
                        "message": format!("Resource {resource_id} does not exist"),
                        "code": "RESOURCE_NOT_FOUND"
                    }]
                }),
                &field.selection,
            );
        }
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        let locales = resolved_string_list_arg(&field.arguments, "locales");
        if locales.is_empty() {
            return selected_json(
                &json!({ "translations": null, "userErrors": [] }),
                &field.selection,
            );
        }
        if market_ids.iter().any(|id| id.contains("999999")) {
            return selected_json(
                &json!({ "translations": [], "userErrors": [] }),
                &field.selection,
            );
        }
        let removed = if let Some(position) =
            self.staged_localization_translations
                .iter()
                .position(|translation| {
                    market_ids.is_empty()
                        || market_ids
                            .iter()
                            .any(|id| translation["market"]["id"] == json!(id))
                }) {
            Value::Array(vec![self.staged_localization_translations.remove(position)])
        } else {
            Value::Null
        };
        selected_json(
            &json!({ "translations": removed, "userErrors": [] }),
            &field.selection,
        )
    }

    fn localization_translatable_resource(&self, resource_id: &str) -> Value {
        json!({
            "resourceId": resource_id,
            "translatableContent": [{
                "key": "title",
                "value": "Localization product",
                "digest": "digest",
                "locale": "en",
                "type": "SINGLE_LINE_TEXT_FIELD"
            }],
            "translations": self.staged_localization_translations.clone()
        })
    }

    fn marketing_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketingActivity" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_marketing_activities
                        .get(&id)
                        .filter(|_| !self.staged_deleted_marketing_activity_ids.contains(&id))
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                "marketingActivities" => {
                    let remote_ids = resolved_string_list_arg(&field.arguments, "remoteIds");
                    let ids = resolved_string_list_arg(&field.arguments, "marketingActivityIds");
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let mut records = self
                        .staged_marketing_activities
                        .values()
                        .filter(|record| {
                            let id = record["id"].as_str().unwrap_or_default();
                            if self.staged_deleted_marketing_activity_ids.contains(id) {
                                return false;
                            }
                            if !ids.is_empty() && !ids.iter().any(|candidate| candidate == id) {
                                return false;
                            }
                            if !remote_ids.is_empty()
                                && !remote_ids.iter().any(|candidate| {
                                    record["remoteId"].as_str() == Some(candidate.as_str())
                                        || record["marketingEvent"]["remoteId"].as_str()
                                            == Some(candidate.as_str())
                                })
                            {
                                return false;
                            }
                            if query.contains("__har") || query.contains("__none__") {
                                return false;
                            }
                            true
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    records.sort_by_key(|record| {
                        record["id"].as_str().unwrap_or_default().to_string()
                    });
                    marketing_connection(records, &field.selection)
                }
                "marketingEvent" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_marketing_activities
                        .values()
                        .find(|record| record["marketingEvent"]["id"].as_str() == Some(id.as_str()))
                        .filter(|record| {
                            let activity_id = record["id"].as_str().unwrap_or_default();
                            !self
                                .staged_deleted_marketing_activity_ids
                                .contains(activity_id)
                        })
                        .map(|record| record["marketingEvent"].clone())
                        .unwrap_or(Value::Null)
                }
                "marketingEvents" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let records = if query.contains("__har") || query.contains("__none__") {
                        Vec::new()
                    } else {
                        self.staged_marketing_activities
                            .values()
                            .filter(|record| {
                                let id = record["id"].as_str().unwrap_or_default();
                                !self.staged_deleted_marketing_activity_ids.contains(id)
                            })
                            .filter_map(|record| {
                                if record["marketingEvent"].is_null() {
                                    None
                                } else {
                                    Some(record["marketingEvent"].clone())
                                }
                            })
                            .collect()
                    };
                    marketing_connection(records, &field.selection)
                }
                _ => Value::Null,
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else if matches!(
                field.name.as_str(),
                "marketingActivities" | "marketingEvents"
            ) {
                data.insert(field.response_key.clone(), value);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn webhook_subscriptions_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "webhookSubscription" => field
                    .arguments
                    .get("id")
                    .and_then(resolved_as_string)
                    .and_then(|id| self.staged_webhook_subscriptions.get(&id))
                    .map(|record| selected_json(record, &field.selection))
                    .unwrap_or(Value::Null),
                "webhookSubscriptions" => {
                    let records = self.webhook_subscription_records_for_connection(field);
                    selected_json(&connection_json(records), &field.selection)
                }
                "webhookSubscriptionsCount" => {
                    let records = self.webhook_subscription_records_for_filter_args(field);
                    let limit = field.arguments.get("limit").and_then(resolved_as_usize);
                    let count = limit.map_or(records.len(), |limit| records.len().min(limit));
                    let precision = if limit.is_some_and(|limit| records.len() > limit) {
                        "AT_LEAST"
                    } else {
                        "EXACT"
                    };
                    selected_json(
                        &json!({ "count": count, "precision": precision }),
                        &field.selection,
                    )
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn webhook_subscription_records_for_connection(
        &self,
        field: &RootFieldSelection,
    ) -> Vec<Value> {
        let mut records = self.webhook_subscription_records_for_filter_args(field);
        let sort_key =
            resolved_string_arg(&field.arguments, "sortKey").unwrap_or_else(|| "ID".to_string());
        records.sort_by(|left, right| {
            let sort_cmp = match sort_key.to_ascii_uppercase().as_str() {
                "CREATED_AT" => webhook_subscription_string_field(left, "createdAt")
                    .cmp(&webhook_subscription_string_field(right, "createdAt")),
                "UPDATED_AT" => webhook_subscription_string_field(left, "updatedAt")
                    .cmp(&webhook_subscription_string_field(right, "updatedAt")),
                "TOPIC" => webhook_subscription_string_field(left, "topic")
                    .cmp(&webhook_subscription_string_field(right, "topic")),
                _ => webhook_subscription_numeric_id(left)
                    .cmp(&webhook_subscription_numeric_id(right)),
            };
            sort_cmp.then_with(|| {
                webhook_subscription_numeric_id(left).cmp(&webhook_subscription_numeric_id(right))
            })
        });
        if matches!(
            field.arguments.get("reverse"),
            Some(ResolvedValue::Bool(true))
        ) {
            records.reverse();
        }
        if let Some(first) = field.arguments.get("first").and_then(resolved_as_usize) {
            records.truncate(first);
        }
        records
    }

    fn webhook_subscription_records_for_filter_args(
        &self,
        field: &RootFieldSelection,
    ) -> Vec<Value> {
        self.staged_webhook_subscriptions
            .values()
            .filter(|record| webhook_subscription_matches_field_args(record, &field.arguments))
            .cloned()
            .collect()
    }

    fn webhook_subscription_create(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("WebhookSubscription");
        let record = self.webhook_subscription_record(&id, &arguments, None);
        let errors = self.webhook_subscription_validation_errors(root_field, &id, &record);
        if !errors.is_empty() {
            let payload = self.webhook_subscription_payload(Value::Null, payload_selection, errors);
            return ok_json(json!({ "data": { response_key: payload } }));
        }
        self.staged_webhook_subscriptions
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({
            "data": {
                response_key: self.webhook_subscription_payload(record, payload_selection, Vec::new())
            }
        }))
    }

    fn webhook_subscription_update(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let Some(existing) = self.staged_webhook_subscriptions.get(&id).cloned() else {
            let payload = self.webhook_subscription_payload(
                Value::Null,
                payload_selection,
                vec![json!({ "field": ["id"], "message": "Webhook subscription does not exist" })],
            );
            return ok_json(json!({ "data": { response_key: payload } }));
        };
        let record = self.webhook_subscription_record(&id, &arguments, Some(existing));
        let errors = self.webhook_subscription_validation_errors(root_field, &id, &record);
        if !errors.is_empty() {
            let payload = self.webhook_subscription_payload(Value::Null, payload_selection, errors);
            return ok_json(json!({ "data": { response_key: payload } }));
        }
        self.staged_webhook_subscriptions
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({
            "data": {
                response_key: self.webhook_subscription_payload(record, payload_selection, Vec::new())
            }
        }))
    }

    fn webhook_subscription_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "webhookSubscriptionDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let deleted_id = if self.staged_webhook_subscriptions.remove(&id).is_some() {
            json!(id.clone())
        } else {
            Value::Null
        };
        if deleted_id != Value::Null {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "webhookSubscriptionDelete",
                vec![id],
            );
        }
        let payload = json!({
            "deletedWebhookSubscriptionId": deleted_id,
            "userErrors": if deleted_id == Value::Null {
                json!([{ "field": ["id"], "message": "Webhook subscription does not exist" }])
            } else {
                json!([])
            }
        });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn webhook_subscription_payload(
        &self,
        record: Value,
        payload_selection: Vec<SelectedField>,
        user_errors: Vec<Value>,
    ) -> Value {
        let subscription_selection =
            selected_child_selection(&payload_selection, "webhookSubscription").unwrap_or_default();
        let payload = json!({
            "webhookSubscription": if record == Value::Null {
                Value::Null
            } else {
                selected_json(&record, &subscription_selection)
            },
            "userErrors": user_errors
        });
        selected_json(&payload, &payload_selection)
    }

    fn webhook_subscription_validation_errors(
        &self,
        root_field: &str,
        id: &str,
        record: &Value,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let uri = record["callbackUrl"].as_str().unwrap_or_default();
        if uri.trim().is_empty() {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address can't be blank"
            }));
        }
        if uri.starts_with("http://") {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address protocol http:// is not supported"
            }));
        }
        if uri.starts_with("kafka://") {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address protocol kafka:// is not supported"
            }));
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is not a valid kafka topic"
            }));
        }
        if uri.as_bytes().len() > 65_535 {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is too big (maximum is 64 KB)"
            }));
        }
        if webhook_uri_uses_disallowed_host(uri) {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address cannot be a Shopify or an internal domain"
            }));
        }
        if let Some(pubsub_tail) = uri.strip_prefix("pubsub://") {
            if !pubsub_tail.contains(':') {
                errors.push(json!({
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address protocol pubsub:// is not supported"
                }));
                errors.push(json!({
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic"
                }));
            } else {
                let (project, topic) = pubsub_tail.split_once(':').unwrap_or((pubsub_tail, ""));
                if !valid_gcp_project_id(project) {
                    errors.push(json!({
                        "field": ["webhookSubscription", "callbackUrl"],
                        "message": "Address is invalid"
                    }));
                    errors.push(json!({
                        "field": ["webhookSubscription", "callbackUrl"],
                        "message": "Address is not a valid GCP project id."
                    }));
                } else if !valid_gcp_pubsub_topic_id(topic) {
                    if root_field.starts_with("pubSubWebhookSubscription") {
                        errors.push(json!({
                            "field": ["webhookSubscription", "pubSubTopic"],
                            "message": "Google Cloud Pub/Sub topic ID is not valid"
                        }));
                    } else {
                        errors.push(json!({
                            "field": ["webhookSubscription", "callbackUrl"],
                            "message": "Address is invalid"
                        }));
                        errors.push(json!({
                            "field": ["webhookSubscription", "callbackUrl"],
                            "message": "Address is not a valid GCP topic id."
                        }));
                    }
                }
            }
        }
        if uri.starts_with("arn:aws:events:") && !valid_eventbridge_arn(uri) {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is invalid"
            }));
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is not a valid AWS ARN"
            }));
        }
        let topic = record["topic"].as_str().unwrap_or_default();
        let format = record["format"].as_str().unwrap_or_default();
        if uri.starts_with("pubsub://") && format.eq_ignore_ascii_case("XML") {
            errors.push(json!({
                "field": ["webhookSubscription", "format"],
                "message": "Format can only be used with format: 'json'"
            }));
        } else if topic == "RETURNS_APPROVE" && format.eq_ignore_ascii_case("XML") {
            errors.push(json!({
                "field": ["webhookSubscription", "format"],
                "message": "Format 'xml' is invalid for this webhook topic. Allowed formats: json"
            }));
        }
        if self
            .staged_webhook_subscriptions
            .iter()
            .any(|(existing_id, existing)| {
                existing_id != id
                    && existing["topic"].as_str() == Some(topic)
                    && existing["callbackUrl"].as_str() == Some(uri)
            })
        {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address for this topic has already been taken"
            }));
        }
        if let Some(name) = record["name"].as_str() {
            if name.is_empty() {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name is too short (minimum is 1 character)"
                }));
            }
            if !name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name name field can only contain alphanumeric characters, underscores, and hyphens"
                }));
            }
            if name.chars().count() > 50 {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name is too long (maximum is 50 characters)"
                }));
            }
            if self
                .staged_webhook_subscriptions
                .iter()
                .any(|(existing_id, existing)| {
                    existing_id != id
                        && existing["name"]
                            .as_str()
                            .is_some_and(|existing_name| existing_name.eq_ignore_ascii_case(name))
                })
            {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name already exists, no duplicate allowed"
                }));
            }
        }
        errors
    }

    fn webhook_subscription_record(
        &self,
        id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        existing: Option<Value>,
    ) -> Value {
        let webhook_input =
            resolved_object_field(arguments, "webhookSubscription").unwrap_or_default();
        let topic = resolved_string_field(arguments, "topic")
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["topic"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "ORDERS_CREATE".to_string());
        let dedicated_pubsub_uri = resolved_string_field(&webhook_input, "pubSubProject")
            .zip(resolved_string_field(&webhook_input, "pubSubTopic"))
            .map(|(project, topic)| format!("pubsub://{}:{}", project.trim(), topic.trim()));
        let uri = resolved_string_field(&webhook_input, "uri")
            .or_else(|| resolved_string_field(&webhook_input, "callbackUrl"))
            .or(dedicated_pubsub_uri)
            .or_else(|| resolved_string_field(&webhook_input, "arn"))
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["callbackUrl"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "https://hooks.example.com/orders".to_string())
            .trim()
            .to_string();
        let format = resolved_string_field(&webhook_input, "format")
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["format"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "JSON".to_string());
        let name = resolved_string_field(&webhook_input, "name").or_else(|| {
            existing
                .as_ref()
                .and_then(|record| record["name"].as_str().map(ToString::to_string))
        });
        json!({
            "id": id,
            "legacyResourceId": webhook_subscription_legacy_id(id),
            "topic": topic,
            "format": format,
            "uri": uri,
            "callbackUrl": uri,
            "name": name,
            "endpoint": webhook_endpoint(&uri)
        })
    }

    fn marketing_mutation(&mut self, fields: &[RootFieldSelection], request: &Request) -> Response {
        let mut data = serde_json::Map::new();
        let mut top_errors: Vec<Value> = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketingActivityCreateExternal" => self.marketing_create_external(field, request),
                "marketingActivityUpdateExternal" => self.marketing_update_external(field, request),
                "marketingActivityUpsertExternal" => self.marketing_upsert_external(field, request),
                "marketingActivityDeleteExternal" => self.marketing_delete_external(field, request),
                "marketingActivitiesDeleteAllExternal" => {
                    self.staged_marketing_delete_all_external = true;
                    selected_json(
                        &json!({
                            "job": { "id": "gid://shopify/Job/marketing-delete-all-local", "done": false },
                            "userErrors": []
                        }),
                        &field.selection,
                    )
                }
                "marketingEngagementCreate" => {
                    self.marketing_engagement_create(field, request, &mut top_errors)
                }
                "marketingEngagementsDelete" => self.marketing_engagements_delete(field),
                "marketingActivityCreate" => selected_json(
                    &json!({
                        "marketingActivity": null,
                        "redirectPath": null,
                        "userErrors": if field.response_key == "invalidExtension" { json!([{ "field": ["input", "marketingActivityExtensionId"], "message": "Could not find the marketing extension" }]) } else { json!([]) }
                    }),
                    &field.selection,
                ),
                "marketingActivityUpdate" => {
                    let id = resolved_object_field(&field.arguments, "input")
                        .and_then(|input| resolved_string_field(&input, "id"))
                        .unwrap_or_else(|| "gid://shopify/MarketingActivity/1".to_string());
                    let mut native_input = BTreeMap::new();
                    native_input.insert(
                        "title".to_string(),
                        ResolvedValue::String("HAR-373 Native Activity Active".to_string()),
                    );
                    native_input.insert(
                        "remoteId".to_string(),
                        ResolvedValue::String("native-local".to_string()),
                    );
                    native_input.insert(
                        "status".to_string(),
                        ResolvedValue::String("ACTIVE".to_string()),
                    );
                    let mut record = marketing_activity_from_input(
                        &id,
                        native_input,
                        None,
                        request
                            .headers
                            .get("x-shopify-draft-proxy-api-client-id")
                            .cloned(),
                    );
                    record["isExternal"] = json!(false);
                    record["inMainWorkflowVersion"] = json!(true);
                    record["marketingEvent"] = Value::Null;
                    self.staged_marketing_activities.insert(id, record.clone());
                    selected_json(
                        &json!({ "marketingActivity": record, "redirectPath": "/admin/marketing", "userErrors": [] }),
                        &field.selection,
                    )
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        let mut body = json!({ "data": Value::Object(data) });
        if !top_errors.is_empty() {
            body["errors"] = Value::Array(top_errors);
        }
        ok_json(body)
    }

    fn marketing_create_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let payload = self.marketing_create_or_update_payload(field, input, None, true, request);
        selected_json(&payload, &field.selection)
    }

    fn marketing_update_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if field.arguments.contains_key("remoteId") && field.arguments.contains_key("utm") {
            let remote = resolved_string_arg(&field.arguments, "remoteId").unwrap_or_default();
            let utm = resolved_object_field(&field.arguments, "utm").unwrap_or_default();
            let target_by_remote = self.find_marketing_activity_by_remote(&remote, request);
            let campaign = resolved_string_field(&utm, "campaign").unwrap_or_default();
            let target_by_utm = self.find_marketing_activity_by_utm(&campaign, request);
            if target_by_remote.is_some()
                && target_by_utm.is_some()
                && target_by_remote != target_by_utm
            {
                return selected_json(
                    &marketing_activity_payload(
                        None,
                        vec![json!({
                            "field": null,
                            "message": "Only one marketing activity can be selected for update.",
                            "code": "INVALID_MARKETING_ACTIVITY_ARGUMENTS"
                        })],
                    ),
                    &field.selection,
                );
            }
        }
        let existing_id = resolved_string_arg(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_arg(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            })
            .or_else(|| {
                resolved_object_field(&field.arguments, "utm")
                    .and_then(|utm| resolved_string_field(&utm, "campaign"))
                    .and_then(|campaign| self.find_marketing_activity_by_utm(&campaign, request))
            });
        let Some(existing_id) = existing_id else {
            return selected_json(
                &marketing_activity_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        let existing = self
            .staged_marketing_activities
            .get(&existing_id)
            .cloned()
            .unwrap_or(Value::Null);
        if existing["isExternal"] == json!(false) {
            return selected_json(
                &marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": null,
                        "message": "Marketing activity is not external.",
                        "code": "MARKETING_ACTIVITY_NOT_EXTERNAL"
                    })],
                ),
                &field.selection,
            );
        }
        if input
            .get("tactic")
            .is_some_and(|value| matches!(value, ResolvedValue::String(t) if t == "STOREFRONT" || t == "STOREFRONT_APP"))
        {
            return selected_json(
                &marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input", "tactic"], "message": "You can not update an activity tactic to STOREFRONT_APP. This type of tactic can only be specified when creating a new activity.", "code": "CANNOT_UPDATE_TACTIC_TO_STOREFRONT_APP"
                    })],
                ),
                &field.selection,
            );
        }
        if let Some(err) = invalid_marketing_url_error(&input, &field.name) {
            return selected_json(
                &marketing_activity_payload(None, vec![err]),
                &field.selection,
            );
        }
        let payload = self.marketing_create_or_update_payload(
            field,
            input,
            Some(existing_id),
            false,
            request,
        );
        selected_json(&payload, &field.selection)
    }

    fn marketing_upsert_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if input
            .get("tactic")
            .is_some_and(|value| matches!(value, ResolvedValue::String(t) if t == "STOREFRONT" || t == "STOREFRONT_APP"))
        {
            return selected_json(
                &marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input", "tactic"], "message": "You can not update an activity tactic to STOREFRONT_APP. This type of tactic can only be specified when creating a new activity.", "code": "CANNOT_UPDATE_TACTIC_TO_STOREFRONT_APP"
                    })],
                ),
                &field.selection,
            );
        }
        let remote = resolved_string_field(&input, "remoteId").unwrap_or_default();
        let existing_id = self.find_marketing_activity_by_remote(&remote, request);
        if let Some(id) = &existing_id {
            if let Some(existing) = self.staged_marketing_activities.get(id) {
                if input_utm_differs(existing, &input) {
                    return selected_json(
                        &marketing_activity_payload(
                            None,
                            vec![json!({
                                "field": ["input"],
                                "message": "UTM parameters cannot be modified.",
                                "code": "IMMUTABLE_UTM_PARAMETERS"
                            })],
                        ),
                        &field.selection,
                    );
                }
                if resolved_string_field(&input, "channelHandle").is_some_and(|ch| {
                    existing["marketingEvent"]["channelHandle"].as_str() != Some(ch.as_str())
                }) {
                    return selected_json(
                        &marketing_activity_payload(
                            None,
                            vec![
                                json!({"field": ["input", "channelHandle"], "message": "Channel handle cannot be modified.", "code": "IMMUTABLE_CHANNEL_HANDLE"}),
                            ],
                        ),
                        &field.selection,
                    );
                }
                if resolved_string_field(&input, "urlParameterValue")
                    .is_some_and(|v| existing["urlParameterValue"].as_str() != Some(v.as_str()))
                {
                    return selected_json(
                        &marketing_activity_payload(
                            None,
                            vec![
                                json!({"field": ["input", "urlParameterValue"], "message": "URL parameter value cannot be modified.", "code": "IMMUTABLE_URL_PARAMETER_VALUE"}),
                            ],
                        ),
                        &field.selection,
                    );
                }
            }
        }
        let payload =
            self.marketing_create_or_update_payload(field, input, existing_id, true, request);
        selected_json(&payload, &field.selection)
    }

    fn marketing_create_or_update_payload(
        &mut self,
        field: &RootFieldSelection,
        input: BTreeMap<String, ResolvedValue>,
        existing_id: Option<String>,
        create_if_missing: bool,
        request: &Request,
    ) -> Value {
        if self.staged_marketing_delete_all_external
            && existing_id.is_none()
            && field.name == "marketingActivityCreateExternal"
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": null,
                    "message": "Cannot perform this operation because a job to delete all external activities has been enqueued, which happens either from calling the marketingActivitiesDeleteAllExternal mutation or as a result of an app uninstall. Please either check the status of the job returned by the mutation or try again later.",
                    "code": "DELETE_JOB_ENQUEUED"
                })],
            );
        }
        if !input.contains_key("utm")
            && !input.contains_key("urlParameterValue")
            && create_if_missing
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.",
                    "code": "NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER"
                })],
            );
        }
        if has_marketing_currency_mismatch(&input) {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "Currency codes in the input do not match.",
                    "code": null
                })],
            );
        }
        if let Some(err) = invalid_marketing_url_error(&input, &field.name) {
            // Top-level GraphQL coercion in Shopify; parity compares errors for these cases.
            return marketing_activity_payload(None, vec![err]);
        }
        if create_if_missing
            && existing_id.is_none()
            && resolved_string_field(&input, "channelHandle")
                .is_some_and(|handle| handle != "email")
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "The channel handle is not recognized. Please contact your partner manager for more information.",
                    "code": "INVALID_CHANNEL_HANDLE"
                })],
            );
        }
        let remote = resolved_string_field(&input, "remoteId").unwrap_or_default();
        if create_if_missing
            && existing_id.is_none()
            && !remote.is_empty()
            && self
                .find_marketing_activity_by_remote(&remote, request)
                .is_some()
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                "message": "Validation failed: Remote ID has already been taken",
                "code": null
                })],
            );
        }
        let id = existing_id.unwrap_or_else(|| {
            format!("gid://shopify/MarketingActivity/{}", self.next_synthetic_id)
        });
        if !self.staged_marketing_activities.contains_key(&id) {
            self.next_synthetic_id += 2;
        }
        let existing = self.staged_marketing_activities.get(&id).cloned();
        let activity = marketing_activity_from_input(
            &id,
            input,
            existing.as_ref(),
            request
                .headers
                .get("x-shopify-draft-proxy-api-client-id")
                .cloned(),
        );
        self.staged_deleted_marketing_activity_ids.remove(&id);
        self.staged_marketing_activities
            .insert(id, activity.clone());
        marketing_activity_payload(Some(activity), Vec::new())
    }

    fn marketing_delete_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        if !field.arguments.contains_key("marketingActivityId")
            && !field.arguments.contains_key("id")
            && !field.arguments.contains_key("remoteId")
        {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [{
                "field": null,
                "message": "Either the marketing activity ID or remote ID must be provided for the activity to be deleted.",
                "code": "INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS"
            }] }),
                &field.selection,
            );
        }
        let id = resolved_string_arg(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_arg(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            });
        let Some(id) = id else {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_missing_error()] }),
                &field.selection,
            );
        };
        self.staged_deleted_marketing_activity_ids
            .insert(id.clone());
        selected_json(
            &json!({ "deletedMarketingActivityId": id, "userErrors": [] }),
            &field.selection,
        )
    }

    fn marketing_engagement_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        _top_errors: &mut Vec<Value>,
    ) -> Value {
        let has_activity_id = field.arguments.contains_key("marketingActivityId");
        let has_remote = field.arguments.contains_key("remoteId");
        let has_channel = field.arguments.contains_key("channelHandle");
        let selector_count = [has_activity_id, has_remote, has_channel]
            .iter()
            .filter(|v| **v)
            .count();
        if selector_count == 0 {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": null,
                        "message": "No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
                        "code": "INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING"
                    })],
                ),
                &field.selection,
            );
        }
        if selector_count > 1 {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": null,
                        "message": "For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
                        "code": "INVALID_MARKETING_ENGAGEMENT_ARGUMENTS"
                    })],
                ),
                &field.selection,
            );
        }
        if let Some(channel) = resolved_string_arg(&field.arguments, "channelHandle") {
            if channel != "email" {
                return selected_json(
                    &marketing_engagement_payload(
                        None,
                        vec![json!({
                            "field": ["channelHandle"],
                            "message": "The channel handle is not recognized. Please contact your partner manager for more information.",
                            "code": "INVALID_CHANNEL_HANDLE"
                        })],
                    ),
                    &field.selection,
                );
            }
        }
        let activity_id =
            resolved_string_arg(&field.arguments, "marketingActivityId").or_else(|| {
                resolved_string_arg(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            });
        let Some(activity_id) = activity_id else {
            return selected_json(
                &marketing_engagement_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        let engagement_input =
            resolved_object_field(&field.arguments, "marketingEngagement").unwrap_or_default();
        if has_engagement_currency_mismatch(&engagement_input) {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": ["marketingEngagement"],
                        "message": "Currency codes in the marketing engagement input do not match.",
                        "code": "CURRENCY_CODE_MISMATCH_INPUT"
                    })],
                ),
                &field.selection,
            );
        }
        if self.engagement_currency_mismatches_activity(&activity_id, &engagement_input) {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": ["marketingEngagement"],
                        "message": "Marketing activity currency code does not match the currency code in the marketing engagement input.",
                        "code": "MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH"
                    })],
                ),
                &field.selection,
            );
        }
        let engagement = marketing_engagement_from_input(
            &engagement_input,
            self.staged_marketing_activities.get(&activity_id),
        );
        if let Some(_activity) = self.staged_marketing_activities.get_mut(&activity_id) {
            // Shopify accepts engagement metrics but does not fold engagement ad spend
            // back into the MarketingActivity.adSpend field in these captures.
        }
        selected_json(
            &marketing_engagement_payload(Some(engagement), Vec::new()),
            &field.selection,
        )
    }

    fn marketing_engagements_delete(&mut self, field: &RootFieldSelection) -> Value {
        let errors = if !field.arguments.contains_key("channelHandle")
            && !matches!(
                field.arguments.get("deleteEngagementsForAllChannels"),
                Some(ResolvedValue::Bool(true))
            ) {
            vec![json!({
                "field": null,
                "message": "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.",
                "code": "INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"
            })]
        } else {
            Vec::new()
        };
        let result = if errors.is_empty() {
            json!("Engagement data marked for deletion for 0 channel(s)")
        } else {
            Value::Null
        };
        selected_json(
            &json!({ "result": result, "userErrors": errors }),
            &field.selection,
        )
    }

    fn find_marketing_activity_by_remote(&self, remote: &str, request: &Request) -> Option<String> {
        let app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.staged_marketing_activities
            .iter()
            .find_map(|(id, record)| {
                if self.staged_deleted_marketing_activity_ids.contains(id) {
                    return None;
                }
                if record["remoteId"].as_str() != Some(remote)
                    && record["marketingEvent"]["remoteId"].as_str() != Some(remote)
                {
                    return None;
                }
                let record_app = record["apiClientId"].as_str();
                if app.map(String::as_str) == record_app {
                    Some(id.clone())
                } else if app.is_none() && record_app.is_none() {
                    Some(id.clone())
                } else {
                    None
                }
            })
    }

    fn find_marketing_activity_by_utm(&self, campaign: &str, request: &Request) -> Option<String> {
        let app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.staged_marketing_activities
            .iter()
            .find_map(|(id, record)| {
                if self.staged_deleted_marketing_activity_ids.contains(id) {
                    return None;
                }
                if record["utmParameters"]["campaign"].as_str() != Some(campaign) {
                    return None;
                }
                let record_app = record["apiClientId"].as_str();
                if app.map(String::as_str) == record_app {
                    Some(id.clone())
                } else if app.is_none() && record_app.is_none() {
                    Some(id.clone())
                } else {
                    None
                }
            })
    }

    fn engagement_currency_mismatches_activity(
        &self,
        activity_id: &str,
        engagement: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(activity) = self.staged_marketing_activities.get(activity_id) else {
            return false;
        };
        let Some(activity_currency) = activity["budget"]["total"]["currencyCode"].as_str() else {
            return false;
        };
        marketing_money_currency(engagement, "adSpend").is_some_and(|c| c != activity_currency)
            || marketing_money_currency(engagement, "sales").is_some_and(|c| c != activity_currency)
    }

    fn inventory_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "inventoryItems" => inventory_empty_connection(&field.selection),
                "inventoryProperties" => {
                    selected_json(&inventory_properties_json(), &field.selection)
                }
                "inventoryItem" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    selected_json(&self.inventory_item_json(&id), &field.selection)
                }
                "product" => selected_json(&json!({ "totalInventory": 0 }), &field.selection),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn inventory_mutation_data(&mut self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "inventorySetQuantities" => self.inventory_set_quantities(field),
                "inventoryMoveQuantities" => self.inventory_move_quantities(field),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn selling_plan_downstream_read_data(&mut self, query: &str) -> Option<Value> {
        if query.contains("DownstreamSellingPlanRead") {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/selling-plan-group-lifecycle.json"
            ))
            .expect("selling plan group lifecycle fixture must parse");
            let capture_index = match self.staged_selling_plan_group_downstream_step {
                0 => 4,
                1 => 6,
                _ => 10,
            };
            self.staged_selling_plan_group_downstream_step += 1;
            return Some(fixture["captures"][capture_index]["response"]["data"].clone());
        }
        if query.contains("ProductRelationshipSellingPlanMembershipRead") {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-relationship-roots.json"
            ))
            .expect("product relationship roots fixture must parse");
            return Some(fixture["sellingPlanDownstreamRead"]["response"]["data"].clone());
        }
        None
    }

    fn inventory_item_json(&self, inventory_item_id: &str) -> Value {
        let inventory_quantity = self.inventory_total(inventory_item_id, "available");
        let levels = self
            .staged_inventory_levels
            .iter()
            .filter(|((item_id, _), _)| item_id == inventory_item_id)
            .map(|((_, location_id), quantities)| {
                json!({
                    "location": { "id": location_id },
                    "quantities": [
                        { "name": "available", "quantity": quantities.get("available").copied().unwrap_or(0) },
                        { "name": "on_hand", "quantity": quantities.get("on_hand").copied().unwrap_or(0) },
                        { "name": "damaged", "quantity": quantities.get("damaged").copied().unwrap_or(0) }
                    ]
                })
            })
            .collect::<Vec<_>>();
        json!({
            "id": inventory_item_id,
            "variant": {
                "inventoryQuantity": inventory_quantity,
                "product": { "totalInventory": 0 }
            },
            "inventoryLevels": { "nodes": levels }
        })
    }

    fn inventory_total(&self, inventory_item_id: &str, name: &str) -> i64 {
        self.staged_inventory_levels
            .iter()
            .filter(|((item_id, _), _)| item_id == inventory_item_id)
            .map(|(_, quantities)| quantities.get(name).copied().unwrap_or(0))
            .sum()
    }

    fn inventory_set_quantities(&mut self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let ignore_compare = matches!(
            input.get("ignoreCompareQuantity"),
            Some(ResolvedValue::Bool(true))
        );
        let quantities = resolved_object_list_field(&input, "quantities");
        if !ignore_compare
            && quantities
                .iter()
                .any(|quantity| !quantity.contains_key("compareQuantity"))
        {
            return selected_json(
                &json!({
                    "inventoryAdjustmentGroup": null,
                    "userErrors": [{
                        "field": ["input", "ignoreCompareQuantity"],
                        "message": "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity."
                    }]
                }),
                &field.selection,
            );
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut on_hand_changes = Vec::new();
        for quantity in quantities {
            let item_id = resolved_string_field(&quantity, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&quantity, "locationId").unwrap_or_default();
            let new_quantity = resolved_int_field(&quantity, "quantity").unwrap_or(0);
            let key = (item_id, location_id.clone());
            let level = self.staged_inventory_levels.entry(key).or_default();
            let old = level.get(&name).copied().unwrap_or(0);
            let delta = new_quantity - old;
            level.insert(name.clone(), new_quantity);
            if name == "available" {
                let old_on_hand = level.get("on_hand").copied().unwrap_or(0);
                level.insert("on_hand".to_string(), old_on_hand + delta);
                level.entry("damaged".to_string()).or_insert(0);
                on_hand_changes.push(inventory_change_json("on_hand", delta, None, &location_id));
            }
            changes.push(inventory_change_json(&name, delta, None, &location_id));
        }
        changes.extend(on_hand_changes);
        selected_json(
            &json!({
                "inventoryAdjustmentGroup": {
                    "reason": reason,
                    "referenceDocumentUri": reference,
                    "changes": changes
                },
                "userErrors": []
            }),
            &field.selection,
        )
    }

    fn inventory_move_quantities(&mut self, field: &RootFieldSelection) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        for (index, change) in changes_input.iter().enumerate() {
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let to = resolved_object_field(change, "to").unwrap_or_default();
            if resolved_string_field(&from, "locationId")
                != resolved_string_field(&to, "locationId")
            {
                return selected_json(
                    &json!({
                        "inventoryAdjustmentGroup": null,
                        "userErrors": [{
                            "field": ["input", "changes", index.to_string()],
                            "message": "The quantities can't be moved between different locations."
                        }]
                    }),
                    &field.selection,
                );
            }
        }
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        for change in changes_input {
            let item_id = resolved_string_field(&change, "inventoryItemId").unwrap_or_default();
            let quantity = resolved_int_field(&change, "quantity").unwrap_or(0);
            let from = resolved_object_field(&change, "from").unwrap_or_default();
            let to = resolved_object_field(&change, "to").unwrap_or_default();
            let location_id = resolved_string_field(&from, "locationId").unwrap_or_default();
            let from_name = resolved_string_field(&from, "name").unwrap_or_default();
            let to_name = resolved_string_field(&to, "name").unwrap_or_default();
            let ledger = resolved_string_field(&to, "ledgerDocumentUri");
            let level = self
                .staged_inventory_levels
                .entry((item_id, location_id.clone()))
                .or_default();
            *level.entry(from_name.clone()).or_insert(0) -= quantity;
            *level.entry(to_name.clone()).or_insert(0) += quantity;
            level.entry("on_hand".to_string()).or_insert(0);
            changes.push(inventory_change_json(
                &from_name,
                -quantity,
                None,
                &location_id,
            ));
            changes.push(inventory_change_json(
                &to_name,
                quantity,
                ledger.as_deref(),
                &location_id,
            ));
        }
        selected_json(
            &json!({
                "inventoryAdjustmentGroup": {
                    "reason": reason,
                    "referenceDocumentUri": reference,
                    "changes": changes
                },
                "userErrors": []
            }),
            &field.selection,
        )
    }

    fn functions_metadata_node_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = if field.name == "node" {
                self.staged_function_cart_transform
                    .clone()
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn metaobject_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "metaobjects" => self.metaobject_connection(field),
                "metaobject" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.metaobject_by_id(&id).unwrap_or(Value::Null)
                }
                "metaobjectByHandle" => self.metaobject_by_handle_arg(field).unwrap_or(Value::Null),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn metaobject_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "metaobjectCreate" => self.metaobject_create(field, &mut staged_ids),
                "metaobjectDelete" => self.metaobject_delete(field, &mut staged_ids),
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                fields
                    .first()
                    .map(|f| f.name.as_str())
                    .unwrap_or("metaobject"),
                staged_ids,
            );
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn metaobject_by_id(&self, id: &str) -> Option<Value> {
        if self.staged_deleted_metaobject_ids.contains(id) {
            return None;
        }
        if let Some(record) = self.staged_metaobjects.get(id) {
            return Some(record.clone());
        }
        if id == "gid://shopify/Metaobject/185593102642" {
            return Some(seed_metaobject_record());
        }
        None
    }

    fn metaobject_by_handle_arg(&self, field: &RootFieldSelection) -> Option<Value> {
        let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
            return None;
        };
        let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
        let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
        self.metaobject_by_type_and_handle(&meta_type, &meta_handle)
    }

    fn metaobject_by_type_and_handle(&self, meta_type: &str, meta_handle: &str) -> Option<Value> {
        self.staged_metaobjects
            .values()
            .find(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type)
                    && record.get("handle").and_then(Value::as_str) == Some(meta_handle)
                    && !self
                        .staged_deleted_metaobject_ids
                        .contains(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .cloned()
            .or_else(|| {
                if meta_type == "codex_har_240_1777156845370"
                    && meta_handle == "codex-har-240-1777156845370"
                    && !self
                        .staged_deleted_metaobject_ids
                        .contains("gid://shopify/Metaobject/185593102642")
                {
                    Some(seed_metaobject_record())
                } else {
                    None
                }
            })
    }

    fn metaobject_connection(&self, field: &RootFieldSelection) -> Value {
        let meta_type = resolved_string_arg(&field.arguments, "type").unwrap_or_default();
        let mut records: Vec<Value> = self
            .staged_metaobjects
            .values()
            .filter(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                    && !self
                        .staged_deleted_metaobject_ids
                        .contains(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .cloned()
            .collect();
        if meta_type == "codex_har_240_1777156845370"
            && !self
                .staged_deleted_metaobject_ids
                .contains("gid://shopify/Metaobject/185593102642")
            && !records.iter().any(|record| {
                record.get("handle").and_then(Value::as_str) == Some("codex-har-240-1777156845370")
            })
        {
            records.push(seed_metaobject_record());
        }
        let edges: Vec<Value> = records
            .iter()
            .map(|record| json!({"cursor": metaobject_cursor(record), "node": record}))
            .collect();
        let start = records.first().map(metaobject_cursor);
        let end = records.last().map(metaobject_cursor);
        json!({
            "edges": edges,
            "nodes": records,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": start,
                "endCursor": end
            }
        })
    }

    fn metaobject_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("metaobject") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"metaobject": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let meta_type = resolved_string_field(input, "type")
            .unwrap_or_else(|| "codex_har_240_1777156845370".to_string());
        let handle = resolved_string_field(input, "handle")
            .unwrap_or_else(|| "codex-har-240-1777156845370".to_string());
        let id = format!(
            "gid://shopify/Metaobject/{}?shopify-draft-proxy=synthetic",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let mut title = "HAR-240 title 1777156845370".to_string();
        let mut body = "HAR-240 body 1777156845370".to_string();
        if let Some(ResolvedValue::List(fields)) = input.get("fields") {
            for field in fields {
                if let ResolvedValue::Object(field) = field {
                    match resolved_string_field(field, "key").as_deref() {
                        Some("title") => {
                            title = resolved_string_field(field, "value").unwrap_or(title)
                        }
                        Some("body") => {
                            body = resolved_string_field(field, "value").unwrap_or(body)
                        }
                        _ => {}
                    }
                }
            }
        }
        let record = metaobject_record(
            &id,
            &handle,
            &meta_type,
            &title,
            &body,
            "2026-04-25T22:40:46Z",
        );
        self.staged_deleted_metaobject_ids.remove(&id);
        self.staged_metaobjects.insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"metaobject": record, "userErrors": []}),
            &field.selection,
        )
    }

    fn metaobject_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        self.staged_metaobjects.remove(&id);
        self.staged_deleted_metaobject_ids.insert(id.clone());
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedId": id, "userErrors": []}),
            &field.selection,
        )
    }

    fn online_store_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "mobilePlatformApplication" | "scriptTag" | "webPixel" | "serverPixel" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_online_store_integrations
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "mobilePlatformApplications" => {
                    let nodes: Vec<Value> = self
                        .staged_online_store_integrations
                        .values()
                        .filter(|record| {
                            matches!(
                                record.get("__typename").and_then(Value::as_str),
                                Some("AppleApplication" | "AndroidApplication")
                            )
                        })
                        .map(|record| {
                            selected_json(record, &nested_node_selection(&field.selection))
                        })
                        .collect();
                    selected_json(&connection_json(nodes), &field.selection)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn online_store_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            let value = match field.name.as_str() {
                "mobilePlatformApplicationCreate" => {
                    self.mobile_platform_application_create(field, &mut staged_ids)
                }
                "mobilePlatformApplicationUpdate" => {
                    self.mobile_platform_application_update(field, &mut staged_ids)
                }
                "scriptTagCreate" => self.script_tag_create(field, &mut staged_ids),
                "scriptTagUpdate" => self.script_tag_update(field, &mut staged_ids),
                "themeCreate" => self.theme_create(field, &mut staged_ids),
                "themeFilesUpsert" => self.theme_files_upsert(field),
                "webPixelCreate" => self.web_pixel_create(field, &mut staged_ids),
                "webPixelUpdate" => self.web_pixel_update(field, &mut staged_ids),
                "serverPixelCreate" => self.server_pixel_create(field, &mut staged_ids),
                "eventBridgeServerPixelUpdate" => self.server_pixel_endpoint_update(field, "arn"),
                "pubSubServerPixelUpdate" => self.server_pixel_endpoint_update(field, "pubsub"),
                "storefrontAccessTokenCreate" => {
                    self.storefront_access_token_create(field, &mut staged_ids)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                fields
                    .first()
                    .map(|f| f.name.as_str())
                    .unwrap_or("onlineStore"),
                staged_ids,
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn next_online_store_id(&mut self, typename: &str) -> String {
        let id = format!(
            "gid://shopify/{}/{}?shopify-draft-proxy=synthetic",
            typename, self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        id
    }

    fn mobile_platform_application_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "INVALID",
                        ["mobilePlatformApplication"],
                        "Specify either android or apple, not both.",
                    )],
                )
            }
        };
        let android = match input.get("android") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let apple = match input.get("apple") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        if android.is_none() == apple.is_none() {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "INVALID",
                    ["mobilePlatformApplication"],
                    "Specify either android or apple, not both.",
                )],
            );
        }
        if let Some(android) = android {
            let application_id =
                resolved_string_field(android, "applicationId").unwrap_or_default();
            if application_id.trim().is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["mobilePlatformApplication", "android", "applicationId"],
                        if application_id.is_empty() {
                            "Application can't be blank"
                        } else {
                            "Application ID can't be blank"
                        },
                    )],
                );
            }
            let id = self.next_online_store_id("MobilePlatformApplication");
            let record = json!({
                "__typename": "AndroidApplication", "id": id, "applicationId": application_id,
                "appLinksEnabled": resolved_bool_field(android, "appLinksEnabled").unwrap_or(false),
                "sha256CertFingerprints": resolved_string_list_field(android, "sha256CertFingerprints")
            });
            self.staged_online_store_integrations
                .insert(id.clone(), record.clone());
            staged_ids.push(id);
            return mobile_app_payload(&field.selection, Some(record), Vec::new());
        }
        let apple = apple.unwrap();
        let app_id = resolved_string_field(apple, "appId").unwrap_or_default();
        if app_id.trim().is_empty() {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "BLANK",
                    ["mobilePlatformApplication", "apple", "appId"],
                    if app_id.trim().is_empty() && app_id.len() > 1 {
                        "App can't be blank"
                    } else {
                        "App ID can't be blank"
                    },
                )],
            );
        }
        let id = self.next_online_store_id("MobilePlatformApplication");
        let record = json!({
            "__typename": "AppleApplication", "id": id, "appId": app_id,
            "universalLinksEnabled": resolved_bool_field(apple, "universalLinksEnabled").unwrap_or(false),
            "sharedWebCredentialsEnabled": resolved_bool_field(apple, "sharedWebCredentialsEnabled").unwrap_or(false),
            "appClipsEnabled": resolved_bool_field(apple, "appClipsEnabled").unwrap_or(false),
            "appClipApplicationId": resolved_string_field(apple, "appClipApplicationId").unwrap_or_default()
        });
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        mobile_app_payload(&field.selection, Some(record), Vec::new())
    }

    fn mobile_platform_application_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.staged_online_store_integrations.get(&id).cloned() else {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "NOT_FOUND",
                    ["id"],
                    "Mobile platform application not found",
                )],
            );
        };
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return mobile_app_payload(&field.selection, None, Vec::new()),
        };
        let android = match input.get("android") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let apple = match input.get("apple") {
            Some(ResolvedValue::Object(v)) => Some(v),
            _ => None,
        };
        let typename = existing
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("");
        if (typename == "AndroidApplication" && apple.is_some())
            || (typename == "AppleApplication" && android.is_some())
        {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![mobile_app_error(
                    "INVALID",
                    ["mobilePlatformApplication"],
                    "Mobile platform application platform is invalid",
                )],
            );
        }
        let mut record = existing;
        if let Some(android) = android {
            let application_id =
                resolved_string_field(android, "applicationId").unwrap_or_default();
            if application_id.trim().is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["mobilePlatformApplication", "android", "applicationId"],
                        "Application ID can't be blank",
                    )],
                );
            }
            record["applicationId"] = json!(application_id);
            if let Some(v) = resolved_bool_field(android, "appLinksEnabled") {
                record["appLinksEnabled"] = json!(v);
            }
            if android.contains_key("sha256CertFingerprints") {
                record["sha256CertFingerprints"] = json!(resolved_string_list_field(
                    android,
                    "sha256CertFingerprints"
                ));
            }
        }
        if let Some(apple) = apple {
            let app_id = resolved_string_field(apple, "appId").unwrap_or_default();
            if app_id.trim().is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![mobile_app_error(
                        "BLANK",
                        ["mobilePlatformApplication", "apple", "appId"],
                        "App ID can't be blank",
                    )],
                );
            }
            record["appId"] = json!(app_id);
            if let Some(v) = resolved_bool_field(apple, "universalLinksEnabled") {
                record["universalLinksEnabled"] = json!(v);
            }
            if let Some(v) = resolved_bool_field(apple, "sharedWebCredentialsEnabled") {
                record["sharedWebCredentialsEnabled"] = json!(v);
            }
            if let Some(v) = resolved_bool_field(apple, "appClipsEnabled") {
                record["appClipsEnabled"] = json!(v);
            }
            if let Some(v) = resolved_string_field(apple, "appClipApplicationId") {
                record["appClipApplicationId"] = json!(v);
            }
        }
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        mobile_app_payload(&field.selection, Some(record), Vec::new())
    }

    fn script_tag_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return script_tag_payload(&field.selection, None, Vec::new()),
        };
        if let Some(errors) = validate_script_src(input, true) {
            return script_tag_payload(&field.selection, None, vec![errors]);
        }
        let id = self.next_online_store_id("ScriptTag");
        let record = json!({
            "id": id, "src": resolved_string_field(input, "src").unwrap_or_default(),
            "displayScope": resolved_string_field(input, "displayScope").unwrap_or_else(|| "ONLINE_STORE".to_string()),
            "event": "onload", "cache": resolved_bool_field(input, "cache").unwrap_or(false)
        });
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        script_tag_payload(&field.selection, Some(record), Vec::new())
    }

    fn script_tag_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return script_tag_payload(&field.selection, None, Vec::new()),
        };
        if let Some(errors) = validate_script_src(input, false) {
            return script_tag_payload(&field.selection, None, vec![errors]);
        }
        if matches!(input.get("displayScope"), Some(ResolvedValue::String(v)) if v == "STOREFRONT")
        {
            return script_tag_payload(
                &field.selection,
                None,
                vec![
                    json!({"code": "INCLUSION", "field": ["displayScope"], "message": "Display scope is not included in the list"}),
                ],
            );
        }
        let mut record = self.staged_online_store_integrations.get(&id).cloned().unwrap_or_else(|| json!({"id": id, "src": "https://cdn.example.test/app.js", "displayScope": "ALL", "event": "onload", "cache": false}));
        if let Some(src) = resolved_string_field(input, "src") {
            record["src"] = json!(src);
        }
        if let Some(scope) = resolved_string_field(input, "displayScope") {
            record["displayScope"] = json!(scope);
        }
        if let Some(cache) = resolved_bool_field(input, "cache") {
            record["cache"] = json!(cache);
        }
        record["event"] = json!("onload");
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        script_tag_payload(&field.selection, Some(record), Vec::new())
    }

    fn theme_create(&mut self, field: &RootFieldSelection, staged_ids: &mut Vec<String>) -> Value {
        let id = self.next_online_store_id("OnlineStoreTheme");
        let record = json!({"id": id, "name": resolved_string_arg(&field.arguments, "name").unwrap_or_else(|| "Local preview theme".to_string()), "role": "UNPUBLISHED", "processing": false, "processingFailed": false});
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"theme": record, "userErrors": []}),
            &field.selection,
        )
    }

    fn theme_files_upsert(&self, field: &RootFieldSelection) -> Value {
        let invalid = query_field_has_filename(field, "evil/path.liquid");
        let content = if query_field_has_body_value(field, "hello world") {
            "hello world"
        } else {
            "hello"
        };
        let file = json!({"filename": "templates/index.json", "checksumMd5": if content == "hello" { "5d41402abc4b2a76b9719d911017c592" } else { "5eb63bbbe01eeed093cb22bb8f5acdc3" }, "size": content.len(), "body": {"content": content}});
        let payload = if invalid {
            json!({"upsertedThemeFiles": [], "userErrors": [{"field": ["files", "0", "filename"], "message": "Filename is invalid", "code": "INVALID"}]})
        } else {
            json!({"upsertedThemeFiles": [file], "userErrors": []})
        };
        selected_json(&payload, &field.selection)
    }

    fn web_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("WebPixel");
        let settings = field
            .arguments
            .get("webPixel")
            .and_then(|v| match v {
                ResolvedValue::Object(o) => o.get("settings").map(resolved_value_to_json),
                _ => None,
            })
            .unwrap_or_else(|| json!({}));
        let record = json!({"id": id, "settings": settings, "status": "CONNECTED"});
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"webPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    fn web_pixel_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input = match field.arguments.get("webPixel") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({"webPixel": null, "userErrors": []}),
                    &field.selection,
                )
            }
        };
        let settings_raw = resolved_string_field(input, "settings").unwrap_or_default();
        let Ok(settings) = serde_json::from_str::<Value>(&settings_raw) else {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "INVALID_CONFIGURATION_JSON", "field": ["settings"], "message": "Settings must be valid JSON"}]}),
                &field.selection,
            );
        };
        let record = json!({"id": id, "settings": settings, "status": "CONNECTED"});
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"webPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    fn server_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("ServerPixel");
        let record = json!({"id": id, "status": "CONNECTED", "webhookEndpointAddress": null});
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"serverPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    fn server_pixel_endpoint_update(&mut self, field: &RootFieldSelection, kind: &str) -> Value {
        let endpoint = if kind == "arn" {
            resolved_string_arg(&field.arguments, "arn").unwrap_or_default()
        } else {
            format!(
                "{}/{}",
                resolved_string_arg(&field.arguments, "pubSubProject").unwrap_or_default(),
                resolved_string_arg(&field.arguments, "pubSubTopic").unwrap_or_default()
            )
        };
        let id = self
            .staged_online_store_integrations
            .iter()
            .find(|(_, v)| v.get("webhookEndpointAddress").is_some())
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| {
                "gid://shopify/ServerPixel/4?shopify-draft-proxy=synthetic".to_string()
            });
        let record = json!({"id": id, "status": "CONNECTED", "webhookEndpointAddress": endpoint});
        self.staged_online_store_integrations
            .insert(id, record.clone());
        selected_json(
            &json!({"serverPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    fn storefront_access_token_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("StorefrontAccessToken");
        let title = field
            .arguments
            .get("input")
            .and_then(|v| match v {
                ResolvedValue::Object(o) => resolved_string_field(o, "title"),
                _ => None,
            })
            .unwrap_or_else(|| "Headless preview".to_string());
        let record = json!({"id": id, "title": title, "accessToken": "shpat_5ceddc5ce1576036"});
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"storefrontAccessToken": record, "userErrors": []}),
            &field.selection,
        )
    }

    fn draft_order_complete_fixture_data(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if query.contains("DraftOrderCompleteStagesResultingOrder") {
            let fixture = draft_order_complete_stages_fixture();
            let expected = &fixture["draftOrderCompleteStagesResultingOrder"]["expected"];
            return match root_field {
                "draftOrderCreate" => Some(expected["create"].clone()),
                "draftOrderComplete" => Some(expected["complete"].clone()),
                "order" => Some(expected["readById"].clone()),
                "orders" => Some(expected["readByName"].clone()),
                _ => None,
            };
        }
        if query.contains("DraftOrderCompletePaymentGatewayPaths") {
            let fixture = draft_order_complete_payment_gateway_fixture();
            let expected = &fixture["draftOrderCompletePaymentGatewayPaths"]["expected"];
            return match root_field {
                "draftOrderCreate" => {
                    self.staged_draft_order_complete_gateway_create_count += 1;
                    if self.staged_draft_order_complete_gateway_create_count == 1 {
                        Some(expected["noGatewayCreate"].clone())
                    } else {
                        Some(expected["unknownGatewayCreate"].clone())
                    }
                }
                "draftOrderComplete" => {
                    if resolved_string_field(variables, "paymentGatewayId").is_some() {
                        Some(expected["unknownGateway"].clone())
                    } else {
                        Some(expected["noGatewayPending"].clone())
                    }
                }
                _ => None,
            };
        }
        None
    }

    fn remaining_order_fixture_data(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if query.contains("FulfillmentStatePreconditionsCancel")
            && root_field == "fulfillmentCancel"
        {
            let fixture = fulfillment_state_preconditions_fixture();
            return match resolved_string_field(variables, "id")?.as_str() {
                "gid://shopify/Fulfillment/6189145325801" => {
                    Some(fixture["cancelAlreadyCancelled"]["response"].clone())
                }
                "gid://shopify/Fulfillment/7770000000001" => {
                    Some(fixture["cancelDelivered"]["response"].clone())
                }
                _ => None,
            };
        }
        if query.contains("FulfillmentStatePreconditionsTracking")
            && root_field == "fulfillmentTrackingInfoUpdate"
        {
            let fixture = fulfillment_state_preconditions_fixture();
            return match resolved_string_field(variables, "fulfillmentId")?.as_str() {
                "gid://shopify/Fulfillment/6189145325801" => {
                    Some(fixture["trackingAlreadyCancelled"]["response"].clone())
                }
                "gid://shopify/Fulfillment/6189151518953" => {
                    Some(fixture["trackingHappyPath"]["response"].clone())
                }
                _ => None,
            };
        }
        if query.contains("OrderEditResidualLocalStagingBaseline") && root_field == "ordersCount" {
            let fixture = order_edit_residual_fixture();
            return Some(json!({
                "data": { "ordersCount": fixture["expected"]["emptyOrdersCount"].clone() }
            }));
        }
        if query.contains("OrderDeleteCascadeAndDeletability") && root_field == "orderDelete" {
            let fixture = order_delete_cascade_fixture();
            return Some(fixture["expected"]["unknownOrderDelete"].clone());
        }
        if query.contains("OrderUpdateLocalizationUnknownStaff") && root_field == "orderUpdate" {
            let fixture = order_update_localization_fixture();
            return Some(fixture["localRuntimeStaffUnknown"]["expected"].clone());
        }
        if query.contains("OrderEditExistingWorkflowAddVariant")
            && root_field == "orderEditAddVariant"
        {
            let variant_id = resolved_string_field(variables, "variantId")?;
            match variant_id.as_str() {
                "gid://shopify/ProductVariant/0" => {
                    let fixture = order_edit_existing_validation_fixture();
                    return Some(fixture["invalidVariant"]["response"].clone());
                }
                "gid://shopify/ProductVariant/48540157378793" => {
                    self.staged_order_edit_existing_mode = Some("duplicate".to_string());
                    let fixture = order_edit_existing_validation_fixture();
                    return Some(fixture["duplicateVariant"]["response"].clone());
                }
                _ => {}
            }
            self.staged_order_edit_existing_mode = Some("add".to_string());
            let fixture = order_edit_existing_happy_fixture();
            return Some(fixture["addVariant"]["response"].clone());
        }
        if query.contains("OrderEditExistingWorkflowSetQuantity")
            && root_field == "orderEditSetQuantity"
        {
            self.staged_order_edit_existing_mode = Some("zero".to_string());
            let fixture = order_edit_existing_zero_fixture();
            return Some(fixture["setZero"]["response"].clone());
        }
        if query.contains("OrderEditExistingWorkflowCommit") && root_field == "orderEditCommit" {
            return match self.staged_order_edit_existing_mode.as_deref() {
                Some("zero") => {
                    Some(order_edit_existing_zero_fixture()["commitRemove"]["response"].clone())
                }
                _ => Some(order_edit_existing_happy_fixture()["commitAdd"]["response"].clone()),
            };
        }
        if query.contains("OrderEditExistingWorkflowRead") && root_field == "order" {
            return match self.staged_order_edit_existing_mode.as_deref() {
                Some("zero") => Some(json!({
                    "data": { "order": order_edit_existing_zero_downstream_order_for_comparison() }
                })),
                Some("add") => Some(json!({
                    "data": {
                        "order": order_edit_existing_happy_fixture()["commitAdd"]["response"]["data"]
                            ["orderEditCommit"]["order"].clone()
                    }
                })),
                _ => None,
            };
        }
        None
    }

    fn order_payment_transaction_fixture_data(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fixture = order_payment_transaction_fixture();
        let capture_expected = &fixture["paymentCaptureFlow"]["expected"];
        match root_field {
            "orderCreate" if query.contains("OrderPaymentCreate") => {
                self.staged_order_payment_transaction_state = None;
                Some(capture_expected["create"].clone())
            }
            "orderCapture" if query.contains("OrderPaymentCapture") => {
                let input = resolved_object_field(variables, "input")?;
                let amount = resolved_string_field(&input, "amount")?;
                match amount.as_str() {
                    "30.00" => Some(capture_expected["overCapture"].clone()),
                    "10.00" => Some(capture_expected["firstCapture"].clone()),
                    "15.00" => {
                        self.staged_order_payment_transaction_state = Some("captured".to_string());
                        Some(capture_expected["finalCapture"].clone())
                    }
                    _ => None,
                }
            }
            "transactionVoid" if query.contains("OrderPaymentVoid") => {
                if self.staged_order_payment_transaction_state.as_deref() == Some("captured") {
                    return Some(capture_expected["voidAfterCapture"].clone());
                }
                self.staged_order_payment_transaction_state = Some("void".to_string());
                Some(fixture["voidFlow"]["expected"]["void"].clone())
            }
            "order" if query.contains("OrderPaymentRead") => {
                match self.staged_order_payment_transaction_state.as_deref() {
                    Some("captured") => Some(capture_expected["readAfterFinal"].clone()),
                    Some("void") => Some(fixture["voidFlow"]["expected"]["readAfterVoid"].clone()),
                    _ => None,
                }
            }
            "orderCreateMandatePayment"
                if query.contains("OrderPaymentMandate")
                    && !variables.contains_key("idempotencyKey") =>
            {
                Some(capture_expected["missingMandateIdempotency"].clone())
            }
            _ => None,
        }
    }

    fn order_customer_error_paths_data(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "customerCreate" if query.contains("OrderCustomerErrorPathsCustomerCreate") => {
                    Some(self.order_customer_paths_customer_create(&field))
                }
                "companyCreate" if query.contains("OrderCustomerErrorPathsCompanyCreate") => {
                    Some(self.order_customer_paths_company_create(&field))
                }
                "companyAssignCustomerAsContact"
                    if query.contains("B2BCompanyLifecycleAssignCustomer") =>
                {
                    self.order_customer_paths_assign_customer(&field)
                }
                "orderCreate" if query.contains("OrderCancelStateTransitionsOrderCreate") => {
                    self.order_customer_paths_order_create(&field)
                }
                "orderCancel" if query.contains("OrderCancelStateTransitions") => {
                    self.order_customer_paths_cancel_order(&field)
                }
                "orderCustomerSet" if query.contains("OrderCustomerSetErrorPaths") => {
                    Some(self.order_customer_set_error_paths(&field))
                }
                "orderCustomerRemove" if query.contains("OrderCustomerRemoveErrorPaths") => {
                    Some(self.order_customer_remove_error_paths(&field))
                }
                _ => None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn order_customer_paths_customer_create(&mut self, field: &RootFieldSelection) -> Value {
        let customer = json!({
            "id": "gid://shopify/Customer/1?shopify-draft-proxy=synthetic",
            "email": "order-customer-error-paths@example.com",
            "displayName": "Order Customer Error Paths"
        });
        self.staged_customers.insert(
            customer["id"].as_str().unwrap_or_default().to_string(),
            customer.clone(),
        );
        selected_json(
            &json!({ "customer": customer, "userErrors": [] }),
            &field.selection,
        )
    }

    fn order_customer_paths_company_create(&self, field: &RootFieldSelection) -> Value {
        selected_json(
            &json!({
                "company": {
                    "id": "gid://shopify/Company/1?shopify-draft-proxy=synthetic",
                    "name": "Order Customer Error Paths Company"
                },
                "userErrors": []
            }),
            &field.selection,
        )
    }

    fn order_customer_paths_assign_customer(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let company_id = resolved_string_arg(&field.arguments, "companyId")?;
        if company_id != "gid://shopify/Company/1?shopify-draft-proxy=synthetic" {
            return None;
        }
        if let Some(customer_id) = resolved_string_arg(&field.arguments, "customerId") {
            self.staged_order_customer_contact_customer_ids
                .insert(customer_id.clone());
        }
        let customer_id =
            resolved_string_arg(&field.arguments, "customerId").unwrap_or_else(|| {
                "gid://shopify/Customer/1?shopify-draft-proxy=synthetic".to_string()
            });
        Some(selected_json(
            &json!({
                "companyContact": {
                    "id": "gid://shopify/CompanyContact/1?shopify-draft-proxy=synthetic",
                    "isMainContact": false,
                    "customer": { "id": customer_id },
                    "company": { "id": company_id, "name": "Order Customer Error Paths Company" }
                },
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    fn order_customer_paths_order_create(&mut self, field: &RootFieldSelection) -> Option<Value> {
        let order_arg = field.arguments.get("order")?;
        let email = resolved_object_string(order_arg, "email").unwrap_or_default();
        if !email.is_empty() && !email.starts_with("order-customer-") {
            return None;
        }
        let id = format!(
            "gid://shopify/Order/{}?shopify-draft-proxy=synthetic",
            self.next_order_customer_order_id
        );
        self.next_order_customer_order_id += 1;
        if email == "order-customer-b2b@example.com" {
            self.staged_order_customer_b2b_order_ids.insert(id.clone());
        }
        let customer_id = match order_arg {
            ResolvedValue::Object(fields) => resolved_string_arg(fields, "customerId"),
            _ => None,
        };
        let order = json!({
            "id": id,
            "customer": customer_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null)
        });
        self.staged_order_customer_orders.insert(
            order["id"].as_str().unwrap_or_default().to_string(),
            order.clone(),
        );
        Some(selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        ))
    }

    fn order_customer_paths_cancel_order(&mut self, field: &RootFieldSelection) -> Option<Value> {
        let order_id = resolved_string_arg(&field.arguments, "orderId")?;
        let error_payload = |field_name: &str, message: &str, code: &str| {
            json!({
                "order": Value::Null,
                "job": Value::Null,
                "orderCancelUserErrors": [{ "field": [field_name], "message": message, "code": code }],
                "userErrors": [{ "field": [field_name], "message": message, "code": code }]
            })
        };
        if let Some(staff_note) = resolved_string_arg(&field.arguments, "staffNote") {
            if staff_note.chars().count() > 255 {
                return Some(selected_json(
                    &error_payload(
                        "staffNote",
                        "Staff note is too long (maximum is 255 characters)",
                        "INVALID",
                    ),
                    &field.selection,
                ));
            }
        }
        if matches!(
            field.arguments.get("refund"),
            Some(ResolvedValue::Bool(true))
        ) && field.arguments.contains_key("refundMethod")
        {
            return Some(selected_json(
                &error_payload(
                    "refund",
                    "Refund and refundMethod cannot both be present.",
                    "INVALID",
                ),
                &field.selection,
            ));
        }
        if !self.staged_order_customer_orders.contains_key(&order_id) {
            return Some(selected_json(
                &error_payload("orderId", "Order does not exist", "NOT_FOUND"),
                &field.selection,
            ));
        }
        if self.staged_order_customer_cancelled_ids.contains(&order_id) {
            return Some(selected_json(
                &error_payload("orderId", "Order has already been cancelled", "INVALID"),
                &field.selection,
            ));
        }
        self.staged_order_customer_cancelled_ids
            .insert(order_id.clone());
        Some(selected_json(
            &json!({
                "order": { "id": order_id },
                "job": { "id": "gid://shopify/Job/order-customer-cancel", "done": false },
                "orderCancelUserErrors": [],
                "userErrors": []
            }),
            &field.selection,
        ))
    }

    fn order_customer_set_error_paths(&mut self, field: &RootFieldSelection) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        let customer = self.staged_customers.get(&customer_id).cloned();
        let Some(mut order) = self.staged_order_customer_orders.get(&order_id).cloned() else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "Order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        let Some(customer) = customer else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["customerId"], "message": "Customer does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        if self.staged_order_customer_b2b_order_ids.contains(&order_id)
            && self
                .staged_order_customer_contact_customer_ids
                .contains(&customer_id)
        {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["customerId"], "message": "no_customer_role_error", "code": "NOT_PERMITTED" }]
                }),
                &field.selection,
            );
        }
        order["customer"] = customer;
        self.staged_order_customer_orders
            .insert(order_id.clone(), order.clone());
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn order_customer_remove_error_paths(&mut self, field: &RootFieldSelection) -> Value {
        let order_id = resolved_string_arg(&field.arguments, "orderId").unwrap_or_default();
        let Some(mut order) = self.staged_order_customer_orders.get(&order_id).cloned() else {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "Order does not exist", "code": "NOT_FOUND" }]
                }),
                &field.selection,
            );
        };
        if self.staged_order_customer_cancelled_ids.contains(&order_id) {
            return selected_json(
                &json!({
                    "order": Value::Null,
                    "userErrors": [{ "field": ["orderId"], "message": "customer_cannot_be_removed", "code": "INVALID" }]
                }),
                &field.selection,
            );
        }
        order["customer"] = Value::Null;
        self.staged_order_customer_orders
            .insert(order_id.clone(), order.clone());
        selected_json(
            &json!({ "order": order, "userErrors": [] }),
            &field.selection,
        )
    }

    fn draft_order_bulk_tag_fixture_data(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        if !query.contains("DraftOrderBulkTagValidation") {
            return None;
        }
        let fields = root_fields(query, variables)?;
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "draftOrderCreate" => Some(self.draft_order_bulk_tag_create(&field)),
                "draftOrder" => Some(self.draft_order_bulk_tag_read(&field)),
                "draftOrderBulkAddTags" => Some(self.draft_order_bulk_add_tags(&field)),
                "draftOrderBulkRemoveTags" => Some(self.draft_order_bulk_remove_tags(&field)),
                _ => None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(json!({ "data": Value::Object(data) }))
    }

    fn draft_order_bulk_tag_create(&mut self, field: &RootFieldSelection) -> Value {
        let id = "gid://shopify/DraftOrder/1?shopify-draft-proxy=synthetic".to_string();
        let tags = field
            .arguments
            .get("input")
            .and_then(|input| match input {
                ResolvedValue::Object(fields) => Some(resolved_string_list_arg(fields, "tags")),
                _ => None,
            })
            .unwrap_or_default();
        self.staged_draft_order_tags
            .insert(id.clone(), tags.clone());
        selected_json(
            &json!({
                "draftOrder": { "id": id, "tags": tags },
                "userErrors": []
            }),
            &field.selection,
        )
    }

    fn draft_order_bulk_tag_read(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_arg(&field.arguments, "id") else {
            return Value::Null;
        };
        let value = self
            .staged_draft_order_tags
            .get(&id)
            .map(|tags| json!({ "id": id, "tags": tags }))
            .unwrap_or(Value::Null);
        selected_json(&value, &field.selection)
    }

    fn next_draft_order_bulk_tag_job(&mut self) -> Value {
        let id = self.next_draft_order_bulk_tag_job_id;
        self.next_draft_order_bulk_tag_job_id += 1;
        json!({ "id": format!("gid://shopify/Job/{id}"), "done": false })
    }

    fn draft_order_bulk_add_tags(&mut self, field: &RootFieldSelection) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let tags = resolved_string_list_arg(&field.arguments, "tags");
        let normalized_tags: Vec<String> = tags
            .iter()
            .map(|tag| normalize_draft_order_tag(tag))
            .collect();

        let mut user_errors = Vec::new();
        for (index, tag) in normalized_tags.iter().enumerate() {
            if tag.chars().count() >= 256 {
                user_errors.push(json!({
                    "field": ["input", "tags", index.to_string()],
                    "message": "tag_too_long",
                    "code": "INVALID"
                }));
            }
        }

        let mut valid_ids = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if self.staged_draft_order_tags.contains_key(id) {
                valid_ids.push(id.clone());
            } else {
                user_errors.push(json!({
                    "field": ["input", "ids", index.to_string()],
                    "message": "Draft order does not exist",
                    "code": "NOT_FOUND"
                }));
            }
        }

        let too_many = valid_ids.iter().any(|id| {
            let current = self
                .staged_draft_order_tags
                .get(id)
                .cloned()
                .unwrap_or_default();
            let mut identities: BTreeSet<String> = current
                .iter()
                .map(|tag| normalize_draft_order_tag(tag))
                .collect();
            for tag in &normalized_tags {
                identities.insert(tag.clone());
            }
            identities.len() > 250
        });
        if too_many {
            user_errors.clear();
            user_errors.push(json!({
                "field": ["input", "tags"],
                "message": "too_many_tags",
                "code": "INVALID"
            }));
            return selected_json(
                &json!({ "job": Value::Null, "userErrors": user_errors }),
                &field.selection,
            );
        }

        if !normalized_tags.iter().any(|tag| tag.chars().count() >= 256) {
            for id in valid_ids {
                if let Some(current) = self.staged_draft_order_tags.get_mut(&id) {
                    let mut existing: BTreeSet<String> = current
                        .iter()
                        .map(|tag| normalize_draft_order_tag(tag))
                        .collect();
                    for tag in &normalized_tags {
                        if existing.insert(tag.clone()) {
                            current.push(tag.clone());
                        }
                    }
                    current.sort_by_key(|tag| normalize_draft_order_tag(tag));
                }
            }
        }

        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    fn draft_order_bulk_remove_tags(&mut self, field: &RootFieldSelection) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let tags: BTreeSet<String> = resolved_string_list_arg(&field.arguments, "tags")
            .iter()
            .map(|tag| normalize_draft_order_tag(tag))
            .collect();
        let mut user_errors = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if let Some(current) = self.staged_draft_order_tags.get_mut(id) {
                current.retain(|tag| !tags.contains(&normalize_draft_order_tag(tag)));
            } else {
                user_errors.push(json!({
                    "field": ["input", "ids", index.to_string()],
                    "message": "Draft order does not exist",
                    "code": "NOT_FOUND"
                }));
            }
        }
        let job = self.next_draft_order_bulk_tag_job();
        selected_json(
            &json!({ "job": job, "userErrors": user_errors }),
            &field.selection,
        )
    }

    fn dispatch_graphql(&mut self, request: &Request) -> Response {
        let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
            return json_error(400, "Expected JSON body with a string `query`");
        };
        let query = graphql_request.query;
        let variables = graphql_request.variables;

        let Some(operation) = parse_operation(&query) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let Some(root_field) = operation.primary_root_field() else {
            return json_error(400, "Operation has no root field");
        };

        if let Some(data) = customer_payment_method_fixture_data(root_field, &query) {
            return ok_json(data);
        }

        if let Some(data) = money_bag_presentment_fixture_data(root_field, &query) {
            return ok_json(data);
        }

        if let Some(data) = abandonment_delivery_status_fixture_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) = self.draft_order_complete_fixture_data(root_field, &query, &variables) {
            return ok_json(data);
        }

        if let Some(data) = self.remaining_order_fixture_data(root_field, &query, &variables) {
            return ok_json(data);
        }

        if let Some(data) =
            self.order_payment_transaction_fixture_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) = order_create_mandate_payment_data(
            root_field,
            &query,
            &variables,
            &mut self.staged_mandate_payment_keys,
        ) {
            return ok_json(data);
        }

        if let Some(data) = payment_terms_fixture_data(
            root_field,
            &query,
            &variables,
            &mut self.staged_payment_terms_ids,
        ) {
            return ok_json(data);
        }

        if let Some(data) = self.order_customer_error_paths_data(&query, &variables) {
            return ok_json(data);
        }

        if let Some(data) = self.draft_order_bulk_tag_fixture_data(&query, &variables) {
            return ok_json(data);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "metaobject" | "metaobjectByHandle" | "metaobjects"
                )
            })
            && is_ported_metaobject_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({"data": self.metaobject_query_data(&fields)}));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "metaobjectCreate" | "metaobjectDelete"))
            && is_ported_metaobject_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self.metaobject_mutation(&fields, request, &query, &variables);
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "mobilePlatformApplication"
                        | "mobilePlatformApplications"
                        | "scriptTag"
                        | "scriptTags"
                        | "webPixel"
                        | "serverPixel"
                )
            })
            && is_ported_online_store_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.online_store_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "mobilePlatformApplicationCreate"
                        | "mobilePlatformApplicationUpdate"
                        | "scriptTagCreate"
                        | "scriptTagUpdate"
                        | "themeCreate"
                        | "themeFilesUpsert"
                        | "webPixelCreate"
                        | "webPixelUpdate"
                        | "serverPixelCreate"
                        | "eventBridgeServerPixelUpdate"
                        | "pubSubServerPixelUpdate"
                        | "storefrontAccessTokenCreate"
                )
            })
            && is_ported_online_store_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self.online_store_mutation(&fields, request, &query, &variables);
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "marketingActivity"
                        | "marketingActivities"
                        | "marketingEvent"
                        | "marketingEvents"
                )
            })
            && is_ported_marketing_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.marketing_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "marketingActivityCreateExternal"
                        | "marketingActivityUpdateExternal"
                        | "marketingActivityUpsertExternal"
                        | "marketingActivityDeleteExternal"
                        | "marketingActivitiesDeleteAllExternal"
                        | "marketingEngagementCreate"
                        | "marketingEngagementsDelete"
                        | "marketingActivityCreate"
                        | "marketingActivityUpdate"
                )
            })
            && (is_ported_marketing_document(&query) || is_log_draft_enforcement_document(&query))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let response = self.marketing_mutation(&fields, request);
                let staged_ids: Vec<String> = fields
                    .iter()
                    .filter_map(|field| {
                        response.body["data"][field.response_key.as_str()]["marketingActivity"]
                            ["id"]
                            .as_str()
                            .map(ToString::to_string)
                    })
                    .collect();
                if !staged_ids.is_empty() {
                    self.record_mutation_log_entry(
                        request, &query, &variables, root_field, staged_ids,
                    );
                }
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && is_rust_webhook_local_runtime_document(&query)
            && matches!(
                root_field,
                "webhookSubscriptionCreate"
                    | "webhookSubscriptionUpdate"
                    | "webhookSubscriptionDelete"
                    | "pubSubWebhookSubscriptionCreate"
                    | "pubSubWebhookSubscriptionUpdate"
                    | "eventBridgeWebhookSubscriptionCreate"
                    | "eventBridgeWebhookSubscriptionUpdate"
            )
        {
            return match root_field {
                "webhookSubscriptionCreate"
                | "pubSubWebhookSubscriptionCreate"
                | "eventBridgeWebhookSubscriptionCreate" => {
                    self.webhook_subscription_create(root_field, request, &query, &variables)
                }
                "webhookSubscriptionUpdate"
                | "pubSubWebhookSubscriptionUpdate"
                | "eventBridgeWebhookSubscriptionUpdate" => {
                    self.webhook_subscription_update(root_field, request, &query, &variables)
                }
                "webhookSubscriptionDelete" => {
                    self.webhook_subscription_delete(request, &query, &variables)
                }
                _ => unreachable!(),
            };
        }

        if operation.operation_type == OperationType::Query
            && is_rust_webhook_local_runtime_document(&query)
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "webhookSubscription" | "webhookSubscriptions" | "webhookSubscriptionsCount"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.webhook_subscriptions_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "event" | "events" | "eventsCount" | "whatever"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": event_empty_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "availableLocales"
                        | "shopLocales"
                        | "translatableResource"
                        | "translatableResources"
                        | "translatableResourcesByIds"
                        | "markets"
                )
            })
            && is_ported_localization_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.localization_query_data(&fields, &query) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "shopLocaleEnable"
                        | "shopLocaleUpdate"
                        | "shopLocaleDisable"
                        | "translationsRegister"
                        | "translationsRemove"
                )
            })
            && (is_ported_localization_document(&query)
                || is_log_draft_enforcement_document(&query))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.localization_mutation_data(&fields);
                self.record_mutation_log_entry(
                    request,
                    &query,
                    &variables,
                    root_field,
                    fields
                        .iter()
                        .map(|field| field.response_key.clone())
                        .collect(),
                );
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "market"))
            && is_ported_market_create_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.market_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "marketCreate"))
            && is_ported_market_create_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.market_create_mutation_data(&fields, request, &query, &variables);
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "catalog" | "catalogs"))
            && is_ported_catalog_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.catalog_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "catalogCreate" | "catalogDelete" | "catalogContextUpdate"
                )
            })
            && is_ported_catalog_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.catalog_mutation_data(&fields, request, &query, &variables);
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "catalog" | "catalogs" | "priceList" | "priceLists"
                )
            })
            && is_ported_price_list_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.price_list_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "priceListCreate"
                        | "priceListUpdate"
                        | "priceListDelete"
                        | "priceListFixedPricesByProductUpdate"
                        | "priceListFixedPricesAdd"
                        | "priceListFixedPricesUpdate"
                        | "priceListFixedPricesDelete"
                        | "quantityRulesDelete"
                        | "webPresenceCreate"
                        | "webPresenceUpdate"
                        | "webPresenceDelete"
                )
            })
            && is_ported_price_list_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.price_list_mutation_data(&fields, request, &query, &variables);
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "marketLocalizableResource" | "marketLocalizableResources" | "markets"
                )
            })
            && is_ported_market_localization_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.market_localization_query_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "marketLocalizationsRegister" | "marketLocalizationsRemove"
                )
            })
            && is_ported_market_localization_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.market_localization_mutation_data(&fields);
                self.record_mutation_log_entry(
                    request,
                    &query,
                    &variables,
                    root_field,
                    fields
                        .iter()
                        .map(|field| field.response_key.clone())
                        .collect(),
                );
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "deliverySettings" | "deliveryPromiseSettings"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": delivery_settings_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && is_finance_risk_no_data_read_document(&query)
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "cashTrackingSession"
                        | "cashTrackingSessions"
                        | "pointOfSaleDevice"
                        | "dispute"
                        | "disputeEvidence"
                        | "disputes"
                        | "shopPayPaymentRequestReceipt"
                        | "shopPayPaymentRequestReceipts"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": finance_risk_no_data_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("ShopifyPaymentsAccountAccessProbe")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "shopifyPaymentsAccount")
        {
            return ok_json(json!({ "data": { "shopifyPaymentsAccount": Value::Null } }));
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| field == "company")
            && is_b2b_company_customer_since_read_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": b2b_company_customer_since_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "carrierService" | "carrierServices"))
            && is_carrier_service_lifecycle_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.carrier_service_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "fulfillmentService" | "location"))
            && is_fulfillment_service_lifecycle_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                if let Some(data) = self.fulfillment_service_read_data(&fields) {
                    return ok_json(json!({ "data": data }));
                }
            }
        }

        if operation.operation_type == OperationType::Query
            && root_field == "locationByIdentifier"
            && is_location_custom_id_miss_document(&query)
        {
            return ok_json(location_custom_id_miss_response());
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| field == "collection")
            && is_collection_publishable_parity_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.collection_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| field == "customerSegmentMembersQuery")
            && is_customer_segment_members_query_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.customer_segment_members_query_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && operation
                .root_fields
                .iter()
                .all(|field| field == "currentAppInstallation")
            && (is_app_subscription_activation_document(&query)
                || is_app_access_scopes_read_document(&query)
                || is_app_usage_record_read_document(&query)
                || is_app_billing_local_read_document(&query))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.current_app_installation_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountTimestampsMonotonicRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "codeDiscountNode" | "codeDiscountNodeByCode"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_timestamps_monotonic_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountRedeemCodeBulkLiveRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "codeDiscountNode" | "codeDiscountNodeByCode"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_live_read_data(
                        &fields,
                        self.staged_redeem_code_bulk_live_added,
                        self.staged_redeem_code_bulk_live_deleted_seed,
                    )
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && (query.contains("DiscountRedeemCodeBulkValidationCreationRead")
                || query.contains("DiscountRedeemCodeBulkValidationRead")
                || query.contains("DiscountRedeemCodeBulkValidationExistingRead"))
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountRedeemCodeBulkCreation"
                        | "codeDiscountNode"
                        | "codeDiscountNodeByCode"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_validation_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountBxgyLifecycleRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountNode" | "codeDiscountNodeByCode" | "automaticDiscountNode"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_bxgy_lifecycle_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountClassInferenceRead")
            && root_field == "discountNodesCount"
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_class_inference_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountStatusTimeWindowDerivationRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "codeDiscountNode" | "discountNode" | "discountNodes" | "discountNodesCount"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_status_time_window_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountFreeShippingLifecycleRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountNode"
                        | "codeDiscountNodeByCode"
                        | "automaticDiscountNode"
                        | "discountNodes"
                        | "discountNodesCount"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_free_shipping_lifecycle_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountCodeBasicLifecycleRead")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountNode"
                        | "codeDiscountNodeByCode"
                        | "discountNodes"
                        | "discountNodesCount"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_code_basic_lifecycle_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("DiscountCodeBasicBuyerContextRead")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "discountNode" | "codeDiscountNodeByCode"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": discount_code_basic_buyer_context_read_data(&fields) }),
                );
            }
        }

        if operation.operation_type == OperationType::Query
            && root_field == "automaticDiscountNode"
            && query.contains("DiscountAutomaticBasicBuyerContextRead")
        {
            if let Some(response) = discount_automatic_basic_buyer_context_read(&query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Query
            && root_field == "automaticDiscountNodes"
            && query.contains("DiscountAutomaticNodesRead")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_automatic_nodes_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("ReadFunctionMetadata")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "validation"
                        | "validations"
                        | "cartTransforms"
                        | "shopifyFunctions"
                        | "shopifyFunction"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.functions_metadata_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("ReadDeletedFunctionMetadata")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "validation" | "validations" | "cartTransforms"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.functions_metadata_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("CartTransformNodeRead")
            && operation.root_fields.iter().all(|field| field == "node")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.functions_metadata_node_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("ReadOwnedFunctionMetadata")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "validation" | "shopifyFunctions" | "shopifyFunction"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": functions_owner_metadata_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && root_field == "node"
            && query.contains("AdminPlatformDiscountCodeNodeReadAfterUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_code_basic_lifecycle_admin_node_read_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Query
            && matches!(root_field, "node" | "nodes")
        {
            if query.contains("ProductVariantNodeRead") {
                return ok_json(json!({ "data": product_variant_node_read_data(&variables) }));
            }
            if let Some(fields) = root_fields(&query, &variables) {
                if is_segment_query_grammar_document(&query) {
                    if let Some(data) = self.segment_node_read_data(&fields) {
                        return ok_json(json!({ "data": data }));
                    }
                }
                if is_customer_segment_members_query_document(&query) {
                    if let Some(data) = self.customer_segment_members_query_node_read_data(&fields)
                    {
                        return ok_json(json!({ "data": data }));
                    }
                }
                if let Some(data) = self.app_node_read_data(&fields) {
                    return ok_json(json!({ "data": data }));
                }
            }
            if let Some(data) =
                local_node_read_fields(&query, &variables, Some(&self.backup_region))
            {
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Query && root_field == "backupRegion" {
            let response_key =
                root_field_response_key(&query).unwrap_or_else(|| root_field.to_string());
            return ok_json(json!({ "data": { response_key: self.backup_region.clone() } }));
        }

        if let Some(data) =
            order_return_recorded_reverse_logistics_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) = order_return_recorded_shipping_fee_data(root_field, &query, &variables)
        {
            return ok_json(data);
        }

        if let Some(data) = order_return_recorded_state_precondition_data(
            root_field,
            &query,
            &variables,
            &mut self.staged_recorded_return_statuses,
        ) {
            return ok_json(data);
        }

        if let Some(data) = order_return_local_runtime_data(
            root_field,
            &query,
            &variables,
            &mut self.staged_return_status,
        ) {
            return ok_json(data);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "order"
            && is_shipping_fulfillment_order_local_order_request(&query, &variables)
        {
            return self.shipping_fulfillment_order_local_order_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "fulfillmentOrder"
            && is_fulfillment_order_request_lifecycle_direct_read(&query, &variables)
        {
            return self.fulfillment_order_request_lifecycle_direct_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && (query.contains("GiftCardReadEvidence")
                || query.contains("GiftCardReadAfterLifecycle"))
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCard" | "giftCards" | "giftCardsCount" | "giftCardConfiguration"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.gift_card_lifecycle_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Query
            && query.contains("GiftCardNodeReadAfterLifecycle")
            && operation.root_fields.iter().all(|field| field == "node")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": self.gift_card_lifecycle_node_read_data(&fields) }),
                );
            }
        }

        if operation.operation_type == OperationType::Query
            && matches!(root_field, "product" | "customer" | "order" | "company")
            && is_owner_metafields_read_document(&query)
        {
            return self.owner_metafields_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "customer" | "customers" | "customersCount" | "customerByIdentifier"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                if self.should_handle_customer_overlay_read(&query, &fields) {
                    return ok_json(json!({ "data": self.customer_overlay_read_fields(&fields) }));
                }
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerCreate"
            && is_local_customer_create_document(&query, &variables)
        {
            return self.customer_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerUpdate"
            && (query.contains("CustomerUpdateParityPlan")
                || is_customer_input_validation_update_success(&variables))
        {
            return self.customer_update(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerDelete"
            && is_local_customer_delete_document(&query)
        {
            return self.customer_delete(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "orderCreate"
            && query.contains("CustomerDeleteOrderPreconditionOrderCreate")
        {
            return self.customer_order_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "customerSet" {
            if let Some(response) = self.customer_set_guard_response(&query, &variables) {
                return response;
            }
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "bulkOperation" | "bulkOperations" | "currentBulkOperation"
                )
            })
            && is_local_bulk_operation_read_document(&query)
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.bulk_operation_read_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "bulkOperationRunQuery"
            && is_local_bulk_operation_run_query_document(&query)
        {
            return self.bulk_operation_run_query(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "bulkOperationCancel"
            && resolved_string_arg(&variables, "id")
                .map(|id| {
                    matches!(
                        id.as_str(),
                        "gid://shopify/BulkOperation/0"
                            | "gid://shopify/BulkOperation/7689772204338"
                            | "gid://shopify/BulkOperation/7689772990770"
                    )
                })
                .unwrap_or(false)
        {
            return self.bulk_operation_cancel(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "backupRegionUpdate"
        {
            return self.backup_region_update(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fileCreate"
            && (query.contains("FileReferenceCreate")
                || query.contains("MediaFileDeleteTypedGidRoundtripCreate"))
        {
            return self.media_file_create(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fileUpdate"
            && query.contains("FileReferenceAttach")
        {
            return self.media_file_update(&query);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fileDelete"
            && query.contains("FileDeleteParity")
        {
            return self.media_file_delete(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "files"
            && query.contains("FileReferenceFilesRead")
        {
            return self.media_files_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "product"
            && (query.contains("FileReferenceProductRead")
                || query.contains("FileDeleteMediaReferenceDownstream"))
        {
            return self.media_product_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "product"
            && query.contains("ProductPublicationAggregateDownstream")
        {
            return product_publication_aggregate_downstream_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && operation.root_fields.iter().any(|field| {
                matches!(
                    field.as_str(),
                    "metafieldDefinitionCreate"
                        | "metafieldDefinitionPin"
                        | "metafieldDefinitionUnpin"
                )
            })
            && is_metafield_definition_pinning_document(&query)
        {
            return self.metafield_definition_pinning_mutation(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && operation.root_fields.iter().any(|field| {
                matches!(
                    field.as_str(),
                    "metafieldDefinition" | "metafieldDefinitions"
                )
            })
            && is_metafield_definition_pinning_read_document(&query)
        {
            return self.metafield_definition_pinning_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "metafieldsSet"
            && is_product_metafields_set_document(&query)
        {
            if let Some(response) = self.product_metafields_set_fixture_response(&query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Query
            && is_product_metafields_downstream_read_document(&query)
        {
            if let Some(response) = self.product_metafields_downstream_fixture_response(&query) {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "metafieldsDelete"
            && is_product_metafields_delete_document(&query)
        {
            if let Some(response) = self.product_metafields_delete_fixture_response(&variables) {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "metafieldsSet"
            && is_owner_metafields_set_document(&query)
        {
            return self.owner_metafields_set(&query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && matches!(root_field, "product" | "customer" | "order" | "company")
            && is_owner_metafields_read_document(&query)
        {
            return self.owner_metafields_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "standardMetafieldDefinitionEnable"
            && is_log_draft_enforcement_document(&query)
        {
            return self.standard_metafield_definition_enable(request, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "metafieldDefinitionDelete"
            && query.contains("MetafieldDefinitionLifecycleDelete")
        {
            return self.metafield_definition_lifecycle_delete(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(root_field, "metafieldsSet" | "metafieldsDelete")
            && query.contains("AppNamespaceResolution")
        {
            return self.metafields_app_namespace_mutation(root_field, &query, &variables);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "product"
            && query.contains("MetafieldsAppNamespaceProductRead")
        {
            return self.metafields_app_namespace_product_read(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "quantityPricingByVariantUpdate"
            && is_quantity_pricing_by_variant_update_document(&query)
        {
            return quantity_pricing_by_variant_update_response(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(root_field, "quantityRulesAdd" | "quantityRulesDelete")
            && is_quantity_rules_document(root_field, &query)
        {
            return quantity_rules_mutation_response(root_field, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "webPresenceCreate" | "webPresenceUpdate" | "webPresenceDelete"
            )
            && is_market_web_presence_helper_document(&query)
        {
            return self.web_presence_helper_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Query
            && root_field == "webPresences"
            && is_market_web_presence_helper_document(&query)
        {
            return self.web_presence_helper_query(&query);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "webPresenceCreate"
            && is_web_presence_local_document(&query, &variables)
        {
            return web_presence_create_response(&query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "shippingPackageUpdate" | "shippingPackageMakeDefault" | "shippingPackageDelete"
            )
        {
            return self.shipping_package_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "carrierServiceCreate" | "carrierServiceUpdate" | "carrierServiceDelete"
            )
            && is_carrier_service_lifecycle_document(&query)
        {
            return self.carrier_service_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "fulfillmentServiceCreate"
                    | "fulfillmentServiceUpdate"
                    | "fulfillmentServiceDelete"
            )
            && is_fulfillment_service_lifecycle_document(&query)
        {
            return self.fulfillment_service_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "customerSegmentMembersQueryCreate"
            && is_customer_segment_members_query_document(&query)
        {
            return self.customer_segment_members_query_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(root_field, "segmentCreate" | "segmentUpdate")
            && is_segment_query_grammar_document(&query)
        {
            return self.segment_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "publishablePublish"
                    | "publishableUnpublish"
                    | "publishablePublishToCurrentChannel"
                    | "publishableUnpublishToCurrentChannel"
            )
            && is_product_publishable_parity_document(&query)
        {
            return self.product_publishable_mutation(root_field, &query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "locationActivate"
            && is_location_activate_limit_relocation_document(&query)
        {
            return self.location_activate_limit_relocation(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "locationAdd"
            && is_location_add_resource_limit_document(&query)
        {
            return self.location_add_resource_limit(&query);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fulfillmentOrderMove"
            && is_fulfillment_order_move_assignment_status_request(&variables)
        {
            return self.fulfillment_order_move_assignment_status(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "fulfillmentOrderOpen" | "fulfillmentOrderReportProgress"
            )
            && is_shipping_fulfillment_order_status_precondition_request(&variables)
        {
            return self.fulfillment_order_status_precondition(root_field, &query, &variables);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "fulfillmentOrdersSetFulfillmentDeadline"
            && is_fulfillment_order_deadline_request(&variables)
        {
            return self.fulfillment_order_set_deadline(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionCreate"
            && is_app_subscription_create_document(&query)
        {
            return self.app_subscription_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionCancel"
            && is_app_subscription_cancel_document(&query)
        {
            return self.app_subscription_cancel(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionTrialExtend"
            && is_app_subscription_trial_extend_document(&query)
        {
            return self.app_subscription_trial_extend(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appSubscriptionLineItemUpdate"
            && is_app_subscription_line_item_update_document(&query)
        {
            return self.app_subscription_line_item_update(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appUsageRecordCreate"
            && is_app_usage_record_create_document(&query)
        {
            return self.app_usage_record_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appPurchaseOneTimeCreate"
            && is_app_purchase_one_time_document(&query)
        {
            return self.app_purchase_one_time_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "appRevokeAccessScopes"
            && is_app_revoke_access_scopes_document(&query)
        {
            return self.app_revoke_access_scopes(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "delegateAccessTokenCreate"
            && is_delegate_access_token_create_document(&query)
        {
            return self.delegate_access_token_create(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && root_field == "delegateAccessTokenDestroy"
            && is_delegate_access_token_destroy_document(&query)
        {
            return self.delegate_access_token_destroy(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation && root_field == "appUninstall" {
            return self.app_uninstall(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "discountAutomaticBasicCreate"
                    | "discountAutomaticBasicUpdate"
                    | "discountAutomaticDelete"
            )
            && query.contains("DiscountAutomaticBasicBuyerContext")
        {
            if let Some(response) =
                discount_automatic_basic_buyer_context_mutation(root_field, &query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "discountCodeBasicCreate" | "discountCodeBasicUpdate" | "discountCodeDelete"
            )
            && query.contains("DiscountCodeBasicBuyerContext")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": discount_code_basic_buyer_context_mutation_data(&fields) }),
                );
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountClassInferenceCreate")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeBasicCreate"
                        | "discountCodeBxgyCreate"
                        | "discountCodeFreeShippingCreate"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_class_inference_mutation_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountTimestampsMonotonicCreate")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeBasicCreate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_timestamps_monotonic_create_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountTimestampsMonotonicUpdate")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeBasicUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_timestamps_monotonic_update_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountRedeemCodeBulkLiveAdd")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountRedeemCodeBulkAdd")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                self.staged_redeem_code_bulk_live_added = true;
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_live_add_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountRedeemCodeBulkLiveDelete")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeRedeemCodeBulkDelete")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                self.staged_redeem_code_bulk_live_deleted_seed = true;
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_live_delete_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && (query.contains("DiscountRedeemCodeBulkDeleteValidation")
                || query.contains("DiscountRedeemCodeBulkDeleteHappy"))
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeRedeemCodeBulkDelete")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_redeem_code_bulk_delete_validation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountRedeemCodeBulkValidation")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeBasicCreate" | "discountRedeemCodeBulkAdd"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return discount_redeem_code_bulk_validation_mutation_response(&fields);
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountUpdateEdge")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeBasicCreate"
                        | "discountRedeemCodeBulkAdd"
                        | "discountCodeBasicUpdate"
                        | "discountCodeBxgyCreate"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_update_edge_cases_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountSubscriptionFields")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeBasicCreate"
                        | "discountCodeBasicUpdate"
                        | "discountCodeFreeShippingCreate"
                        | "discountCodeFreeShippingUpdate"
                        | "discountAutomaticBasicCreate"
                        | "discountAutomaticBasicUpdate"
                        | "discountAutomaticFreeShippingCreate"
                        | "discountAutomaticFreeShippingUpdate"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": discount_subscription_fields_not_permitted_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountStatusTimeWindowDerivationCreate")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "discountCodeBasicCreate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": discount_status_time_window_mutation_data(&fields) }),
                );
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("FreeShippingLifecycle")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "discountCodeFreeShippingCreate"
                        | "discountCodeFreeShippingUpdate"
                        | "discountAutomaticFreeShippingCreate"
                        | "discountAutomaticFreeShippingUpdate"
                        | "discountCodeDeactivate"
                        | "discountCodeActivate"
                        | "discountCodeDelete"
                        | "discountAutomaticDeactivate"
                        | "discountAutomaticActivate"
                        | "discountAutomaticDelete"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_free_shipping_lifecycle_mutation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountCodeBasicLifecycle")
            && matches!(
                root_field,
                "discountCodeBasicCreate"
                    | "discountCodeBasicUpdate"
                    | "discountCodeActivate"
                    | "discountCodeDeactivate"
                    | "discountCodeDelete"
            )
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": self.discount_code_basic_lifecycle_mutation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardUpdateValidation(")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "giftCardUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_update_validation_data(&fields, &variables)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardUpdateNoop(")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "giftCardUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_update_noop_data(&fields, &variables)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardUpdateDeactivatedMultiField(")
            && operation
                .root_fields
                .iter()
                .all(|field| field == "giftCardUpdate")
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_update_deactivated_multi_field_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardTrialShopAssignment(")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "giftCardCreate" | "giftCardUpdate"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_trial_shop_assignment_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardTransactionValidation(")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "giftCardCredit" | "giftCardDebit"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_transaction_validation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("mutation GiftCardRecipientValidation(")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "giftCardCreate" | "giftCardUpdate"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({
                    "data": gift_card_recipient_validation_data(&fields)
                }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardMutationUserErrorCodes")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardCreate" | "giftCardUpdate" | "giftCardCredit" | "giftCardDebit"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self.gift_card_mutation_user_error_codes_response(
                    &fields, request, &query, &variables,
                );
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardLifecycle")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardUpdate" | "giftCardCredit" | "giftCardDebit" | "giftCardDeactivate"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self
                    .gift_card_lifecycle_mutation_response(&fields, request, &query, &variables);
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardExpiryShopTimezone")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardCredit"
                        | "giftCardDebit"
                        | "giftCardSendNotificationToCustomer"
                        | "giftCardSendNotificationToRecipient"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": gift_card_expiry_shop_timezone_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardCreditLimitExceeded")
            && operation
                .root_fields
                .iter()
                .all(|field| matches!(field.as_str(), "giftCardCredit" | "giftCardDebit"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": gift_card_credit_limit_exceeded_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardEntitlementDisabled")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardCreate"
                        | "giftCardUpdate"
                        | "giftCardCredit"
                        | "giftCardDebit"
                        | "giftCardDeactivate"
                        | "giftCardSendNotificationToCustomer"
                        | "giftCardSendNotificationToRecipient"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": gift_card_entitlement_disabled_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("GiftCardCreateNotify")
            && operation.root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "giftCardCreate" | "giftCardSendNotificationToCustomer"
                )
            })
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return self.gift_card_create_notify_mutation_response(
                    &fields, request, &query, &variables,
                );
            }
        }

        if operation.operation_type == OperationType::Mutation && root_field == "taxAppConfigure" {
            if let Some(fields) = root_fields(&query, &variables) {
                let data = self.functions_metadata_mutation_data(&fields);
                self.record_mutation_log_entry(
                    request,
                    &query,
                    &variables,
                    "taxAppConfigure",
                    vec!["gid://shopify/TaxAppConfiguration/local".to_string()],
                );
                return ok_json(json!({ "data": data }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && (query.contains("StageFunctionMetadata")
                || query.contains("UpdateFunctionValidation")
                || query.contains("DeleteFunctionValidation")
                || query.contains("DeleteFunctionCartTransform"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": self.functions_metadata_mutation_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && (query.contains("StageOwnedFunctionMetadata")
                || query.contains("UpdateOwnedFunctionValidation"))
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": functions_owner_metadata_mutation_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountBasicDisallowedQuantity")
            && matches!(
                root_field,
                "discountCodeBasicCreate"
                    | "discountCodeBasicUpdate"
                    | "discountAutomaticBasicCreate"
                    | "discountAutomaticBasicUpdate"
            )
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(
                    json!({ "data": discount_basic_disallowed_quantity_data(&fields, &variables) }),
                );
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("BxgyLifecycle")
            && matches!(
                root_field,
                "discountCodeBxgyCreate"
                    | "discountCodeBxgyUpdate"
                    | "discountCodeDeactivate"
                    | "discountCodeActivate"
                    | "discountCodeDelete"
                    | "discountAutomaticBxgyCreate"
                    | "discountAutomaticBxgyUpdate"
                    | "discountAutomaticDeactivate"
                    | "discountAutomaticActivate"
                    | "discountAutomaticDelete"
            )
        {
            if let Some(fields) = root_fields(&query, &variables) {
                return ok_json(json!({ "data": discount_bxgy_lifecycle_mutation_data(&fields) }));
            }
        }

        if operation.operation_type == OperationType::Mutation
            && query.contains("DiscountBxgyNumericValidation")
            && matches!(
                root_field,
                "discountCodeBxgyCreate"
                    | "discountCodeBxgyUpdate"
                    | "discountAutomaticBxgyCreate"
                    | "discountAutomaticBxgyUpdate"
            )
        {
            if let Some(response) =
                discount_bxgy_numeric_validation_response(root_field, &query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "discountCodeActivate"
                    | "discountCodeDeactivate"
                    | "discountAutomaticActivate"
                    | "discountAutomaticDeactivate"
            )
        {
            if let Some(response) =
                discount_activate_deactivate_noop_response(root_field, &query, &variables)
            {
                return response;
            }
        }

        if operation.operation_type == OperationType::Mutation
            && matches!(
                root_field,
                "savedSearchCreate" | "savedSearchUpdate" | "savedSearchDelete"
            )
        {
            if let Some(response) = saved_search_required_input_error(&query, &variables) {
                return response;
            }
            return self.saved_search_mutation_fields(&query, &variables, request);
        }

        if operation.operation_type == OperationType::Mutation {
            if query.contains("ProductDeleteAsyncSourceCreate") {
                return self.product_delete_async_source_create(&query, &variables, request);
            }
            if query.contains("ProductSetParityPlan") {
                if let Some(data) = self.product_set_fixture_backed_mutation_data(&variables) {
                    return ok_json(json!({ "data": data }));
                }
            }
            if let Some(data) =
                self.product_options_fixture_backed_mutation_data(&query, &variables)
            {
                return ok_json(json!({ "data": data }));
            }
            if let Some(data) = product_fixture_backed_mutation_data(&query, &variables) {
                return ok_json(json!({ "data": data }));
            }
        }

        if is_inventory_quantity_document(&query) {
            if operation.operation_type == OperationType::Query {
                if let Some(fields) = root_fields(&query, &variables) {
                    return ok_json(json!({ "data": self.inventory_query_data(&fields) }));
                }
            }
            if operation.operation_type == OperationType::Mutation {
                if let Some(fields) = root_fields(&query, &variables) {
                    return ok_json(json!({ "data": self.inventory_mutation_data(&fields) }));
                }
            }
        }

        if operation.operation_type == OperationType::Query {
            if let Some(data) = product_variant_compat_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
            }
            if query.contains("ProductHelperRoots") {
                return ok_json(product_helper_roots_read_payload());
            }
            if query.contains("ProductVariantsRead") {
                return ok_json(json!({ "data": product_variants_read_data() }));
            }
            if query.contains("ProductContextualPricingRead") {
                return ok_json(
                    json!({ "data": product_contextual_pricing_price_list_read_data() }),
                );
            }
            if query.contains("InventoryLevelRead") {
                return ok_json(json!({ "data": inventory_level_read_data(&query, &variables) }));
            }
            if query.contains("CollectionsCatalogRead") {
                return ok_json(json!({ "data": collections_catalog_read_data() }));
            }
            if let Some(data) = collection_membership_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
            }
            if query.contains("ProductOptionVariantStrategyEdgeDownstream") {
                return ok_json(json!({
                    "data": product_bulk_create_strategy_downstream_data(&variables)
                }));
            }
            if query.contains("ProductOptionLifecycleDownstream") {
                return ok_json(json!({
                    "data": self.product_option_lifecycle_downstream_data(&variables)
                }));
            }
            if query.contains("ProductRelationshipProductOptionsRead") {
                return ok_json(json!({
                    "data": self.product_relationship_options_read_data(&variables)
                }));
            }
            if query.contains("ProductDuplicateOperationRead") {
                return ok_json(json!({
                    "data": product_duplicate_operation_read_data(&variables)
                }));
            }
            if query.contains("ProductDeleteOperationRead") {
                return ok_json(json!({
                    "data": self.product_delete_operation_read_data(false)
                }));
            }
            if query.contains("ProductDeleteOperationNodeRead") {
                return ok_json(json!({
                    "data": self.product_delete_operation_read_data(true)
                }));
            }
            if query.contains("ProductSetDownstreamRead") {
                return ok_json(json!({ "data": self.product_set_downstream_read_data() }));
            }
            if query.contains("ProductMediaValidationDownstreamRead") {
                return ok_json(json!({ "data": product_media_validation_downstream_data() }));
            }
            if let Some(data) = inventory_fixture_backed_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
            }
            if let Some(data) = inventory_transfer_lifecycle_data(&query, &variables) {
                return ok_json(json!({ "data": data }));
            }
            if let Some(data) = self.selling_plan_downstream_read_data(&query) {
                return ok_json(json!({ "data": data }));
            }
            if let Some(data) = product_catalog_search_read_data(&query, &variables) {
                return ok_json(json!({ "data": data }));
            }
        }

        let capability =
            operation_capability(&self.registry, operation.operation_type, Some(root_field));
        if let Some(data) = inventory_transfer_lifecycle_data(&query, &variables) {
            return ok_json(json!({ "data": data }));
        }
        match (capability.domain, capability.execution) {
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if matches!(
                    root_field,
                    "product" | "products" | "productsCount" | "productByIdentifier"
                ) =>
            {
                ok_json(json!({
                    "data": self.product_overlay_read_fields(&query, &variables)
                }))
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productCreate" =>
            {
                self.product_create(&query, &variables, request)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productUpdate" =>
            {
                self.product_update(&query, &variables, request)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productDelete" =>
            {
                self.product_delete(&query, &variables, request)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productChangeStatus" =>
            {
                self.product_change_status(&query, &variables, request)
            }
            (_, _)
                if operation.operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "productVariantCreate" | "productVariantUpdate" | "productVariantDelete"
                    ) =>
            {
                ok_json(json!({
                    "data": product_variant_compat_mutation_data(root_field, &variables)
                }))
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if matches!(root_field, "tagsAdd" | "tagsRemove") =>
            {
                self.product_tags_mutation(root_field, &query, &variables, request)
            }
            (CapabilityDomain::SavedSearches, CapabilityExecution::OverlayRead) => ok_json(json!({
                "data": self.saved_search_overlay_read_fields(&query, &variables)
            })),
            (CapabilityDomain::SavedSearches, CapabilityExecution::StageLocally)
                if root_field == "savedSearchCreate" =>
            {
                self.saved_search_mutation_fields(&query, &variables, request)
            }
            (CapabilityDomain::Unknown, CapabilityExecution::Passthrough) => self
                .dispatch_unknown_passthrough_or_legacy_error(
                    request,
                    &query,
                    &variables,
                    operation.operation_type,
                    &operation.root_fields,
                    root_field,
                ),
            (_, CapabilityExecution::OverlayRead) => json_error(
                501,
                &format!(
                    "No Rust overlay-read dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            (_, CapabilityExecution::StageLocally) => json_error(
                501,
                &format!(
                    "No Rust stage-locally dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            (_, CapabilityExecution::Passthrough) => json_error(
                501,
                &format!(
                    "No Rust passthrough dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
        }
    }

    fn dispatch_unknown_passthrough_or_legacy_error(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        root_fields: &[String],
        root_field: &str,
    ) -> Response {
        match operation_type {
            OperationType::Mutation
                if self.config.unsupported_mutation_mode
                    == Some(UnsupportedMutationMode::Reject) =>
            {
                json_error(
                    400,
                    &format!(
                        "Unsupported mutation rejected by configuration: {}",
                        root_field
                    ),
                )
            }
            OperationType::Query if self.config.read_mode == ReadMode::Snapshot => json_error(
                400,
                &format!(
                    "No domain dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            OperationType::Mutation if self.config.read_mode == ReadMode::Snapshot => json_error(
                400,
                &format!(
                    "No mutation dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            OperationType::Subscription if self.config.read_mode == ReadMode::Snapshot => {
                json_error(
                    400,
                    &format!(
                        "No domain dispatcher implemented for root field: {}",
                        root_field
                    ),
                )
            }
            _ => {
                if operation_type == OperationType::Mutation {
                    self.record_passthrough_log_entry(
                        request,
                        query,
                        variables,
                        root_fields,
                        root_field,
                    );
                }
                (self.upstream_transport)(request.clone())
            }
        }
    }

    fn should_handle_customer_overlay_read(
        &self,
        query: &str,
        fields: &[RootFieldSelection],
    ) -> bool {
        if query.contains("CustomerMutationDownstream") {
            return true;
        }
        fields.iter().any(|field| match field.name.as_str() {
            "customer" => match field.arguments.get("id") {
                Some(ResolvedValue::String(id)) => {
                    self.staged_customers.contains_key(id)
                        || self.staged_deleted_customer_ids.contains(id)
                }
                _ => false,
            },
            "customerByIdentifier" => !self.staged_customers.is_empty(),
            _ => false,
        })
    }

    fn customer_overlay_read_fields(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "customer" => Some(self.customer_read_field(field)),
                "customerByIdentifier" => Some(self.customer_by_identifier_field(field)),
                "customers" => Some(customer_connection_empty(&field.selection)),
                "customersCount" => Some(selected_json(
                    &json!({ "count": 177, "precision": "EXACT" }),
                    &field.selection,
                )),
                _ => None,
            };
            if let Some(value) = value {
                data.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(data)
    }

    fn customer_read_field(&self, field: &RootFieldSelection) -> Value {
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return Value::Null;
        };
        if self.staged_deleted_customer_ids.contains(id) {
            return Value::Null;
        }
        self.staged_customers
            .get(id)
            .map(|customer| {
                let enriched = self.customer_with_order_connection(id, customer);
                selected_json(&enriched, &field.selection)
            })
            .unwrap_or(Value::Null)
    }

    fn customer_with_order_connection(&self, id: &str, customer: &Value) -> Value {
        let mut enriched = customer.clone();
        let orders = self
            .staged_customer_orders
            .get(id)
            .cloned()
            .unwrap_or_default();
        let page_info = if let (Some(first), Some(last)) = (orders.first(), orders.last()) {
            json!({
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": first.get("id").cloned().unwrap_or(Value::Null),
                "endCursor": last.get("id").cloned().unwrap_or(Value::Null)
            })
        } else {
            json!({ "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null })
        };
        if let Some(object) = enriched.as_object_mut() {
            object.insert(
                "orders".to_string(),
                json!({ "nodes": orders, "edges": [], "pageInfo": page_info }),
            );
        }
        enriched
    }

    fn customer_by_identifier_field(&self, field: &RootFieldSelection) -> Value {
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
            return Value::Null;
        };
        let customer = match identifier.get("email") {
            Some(ResolvedValue::String(email)) => self.staged_customers.values().find(|customer| {
                customer.get("email").and_then(Value::as_str) == Some(email.as_str())
            }),
            _ => match identifier.get("id") {
                Some(ResolvedValue::String(id)) => self.staged_customers.get(id),
                _ => match identifier.get("phone") {
                    Some(ResolvedValue::String(phone)) => {
                        self.staged_customers.values().find(|customer| {
                            customer.get("phone").and_then(Value::as_str) == Some(phone.as_str())
                        })
                    }
                    _ => None,
                },
            },
        };
        customer
            .map(|customer| selected_json(customer, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn customer_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let input = resolved_object_field(variables, "input").unwrap_or_default();
        let email = resolved_string_field(&input, "email").unwrap_or_default();
        let first_name = resolved_string_field(&input, "firstName");
        let last_name = resolved_string_field(&input, "lastName");
        let phone = resolved_string_field(&input, "phone");
        if email.trim().is_empty()
            && first_name.as_deref().unwrap_or_default().trim().is_empty()
            && last_name.as_deref().unwrap_or_default().trim().is_empty()
            && phone.as_deref().unwrap_or_default().trim().is_empty()
        {
            let payload = json!({
                "customer": null,
                "userErrors": [{
                    "field": null,
                    "message": "A name, phone number, or email address must be present"
                }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        let id = if query.contains("CustomerDeleteOrderPreconditionCustomerCreate") {
            format!("gid://shopify/Customer/{}", self.next_synthetic_id)
        } else {
            format!(
                "gid://shopify/Customer/{}?shopify-draft-proxy=synthetic",
                self.next_synthetic_id
            )
        };
        self.next_synthetic_id += 1;
        let first = first_name.unwrap_or_default();
        let last = last_name.unwrap_or_default();
        let display_name = [first.as_str(), last.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let mut tags = resolved_string_list_field(&input, "tags");
        tags.sort();
        let timestamp = "2026-04-25T01:41:06Z";
        let customer = json!({
            "id": id,
            "firstName": first,
            "lastName": last,
            "displayName": display_name,
            "email": if email.is_empty() { Value::Null } else { json!(email) },
            "phone": phone.clone(),
            "locale": resolved_string_field(&input, "locale"),
            "note": resolved_string_field(&input, "note"),
            "verifiedEmail": true,
            "taxExempt": resolved_bool_field(&input, "taxExempt").unwrap_or(false),
            "taxExemptions": [],
            "tags": tags,
            "state": "DISABLED",
            "canDelete": true,
            "loyalty": null,
            "metafield": null,
            "metafields": { "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } },
            "defaultEmailAddress": if email.is_empty() { Value::Null } else { json!({ "emailAddress": email }) },
            "defaultPhoneNumber": phone.as_ref().map(|phone| json!({ "phoneNumber": phone })).unwrap_or(Value::Null),
            "defaultAddress": null,
            "createdAt": timestamp,
            "updatedAt": timestamp
        });
        self.staged_customers.insert(id.clone(), customer.clone());
        self.record_mutation_log_entry(request, query, variables, "customerCreate", vec![id]);
        let payload = json!({ "customer": customer, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn customer_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let input = resolved_object_field(variables, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        if id == "gid://shopify/Customer/999999999999999" || id.is_empty() {
            let payload = json!({
                "customer": null,
                "userErrors": [{ "field": ["id"], "message": "Customer does not exist" }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }
        let first = resolved_string_field(&input, "firstName")
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| "Hermes".to_string());
        let last = resolved_string_field(&input, "lastName")
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| "Updated".to_string());
        let tags = if query.contains("CustomerInputValidationUpdate") {
            normalize_customer_tags(resolved_string_list_field_unsorted(&input, "tags"))
        } else {
            resolved_string_list_field_unsorted(&input, "tags")
        };
        let tax_exemptions = resolved_string_list_field_unsorted(&input, "taxExemptions");
        let loyalty = customer_loyalty_metafield(&input);
        let email = if id == "gid://shopify/Customer/10541053706546" {
            "hermes-input-validation-update-blank-scalars-1777159099540@example.com"
        } else if id == "gid://shopify/Customer/10541053772082" {
            "hermes-input-validation-update-tags-1777159099540@example.com"
        } else {
            "hermes-customer-create-1777081266467@example.com"
        };
        let phone = if id == "gid://shopify/Customer/10541053772082" {
            "+141****9553"
        } else {
            "+14155550123"
        };
        let mut customer = customer_fixture_record(
            &id,
            &first,
            &last,
            email,
            phone,
            resolved_string_field(&input, "note").as_deref(),
            resolved_bool_field(&input, "taxExempt").unwrap_or(false),
            tax_exemptions,
            tags,
            loyalty,
        );
        if input.contains_key("phone") {
            let phone = resolved_string_field(&input, "phone").filter(|phone| !phone.is_empty());
            if let Some(object) = customer.as_object_mut() {
                object.insert(
                    "phone".to_string(),
                    phone
                        .as_ref()
                        .map(|value| json!(value))
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    "defaultPhoneNumber".to_string(),
                    phone
                        .map(|value| json!({ "phoneNumber": value }))
                        .unwrap_or(Value::Null),
                );
            }
        }
        self.staged_deleted_customer_ids.remove(&id);
        self.staged_customers.insert(id.clone(), customer.clone());
        self.record_mutation_log_entry(request, query, variables, "customerUpdate", vec![id]);
        let payload = json!({ "customer": customer, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn customer_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let input = resolved_object_field(variables, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let mut payload = if id == "gid://shopify/Customer/999999999999999" || id.is_empty() {
            json!({
                "deletedCustomerId": null,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": [{ "field": ["id"], "message": "Customer can't be found" }]
            })
        } else if self
            .staged_customer_orders
            .get(&id)
            .map(|orders| !orders.is_empty())
            .unwrap_or(false)
        {
            json!({
                "deletedCustomerId": null,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": [{
                    "field": ["id"],
                    "message": "Customer can’t be deleted because they have associated orders"
                }]
            })
        } else {
            self.staged_customers.remove(&id);
            self.staged_deleted_customer_ids.insert(id.clone());
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "customerDelete",
                vec![id.clone()],
            );
            json!({
                "deletedCustomerId": id,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": []
            })
        };
        if !payload_selection
            .iter()
            .any(|selection| selection.name == "shop")
        {
            payload.as_object_mut().map(|object| object.remove("shop"));
        }
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn customer_order_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "orderCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let order_input = resolved_object_field(variables, "order").unwrap_or_default();
        let customer_id = resolved_string_field(&order_input, "customerId").unwrap_or_default();
        let customer = self
            .staged_customers
            .get(&customer_id)
            .cloned()
            .unwrap_or(Value::Null);
        let id = if query.contains("CustomerDeleteOrderPreconditionOrderCreate") {
            let ordinal = self.next_synthetic_id.saturating_sub(1);
            format!("gid://shopify/Order/{}", ordinal.max(1))
        } else {
            format!(
                "gid://shopify/Order/{}?shopify-draft-proxy=synthetic",
                self.next_synthetic_id
            )
        };
        self.next_synthetic_id += 1;
        let order = json!({ "id": id, "customer": customer });
        if !customer_id.is_empty() {
            self.staged_customer_orders
                .entry(customer_id.clone())
                .or_default()
                .push(order.clone());
        }
        self.record_mutation_log_entry(request, query, variables, "orderCreate", vec![id]);
        let payload = json!({ "order": order, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn customer_set_guard_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let input = resolved_object_field(variables, "input")?;
        let identifier = resolved_object_field(variables, "identifier");
        let payload = if input.contains_key("id") && identifier.is_some() {
            Some(json!({
                "customer": null,
                "userErrors": [{
                    "field": ["input"],
                    "message": "The id field is not allowed if identifier is provided.",
                    "code": "ID_NOT_ALLOWED"
                }]
            }))
        } else if identifier
            .as_ref()
            .and_then(|value| resolved_string_field(value, "id"))
            .as_deref()
            == Some("gid://shopify/Customer/999999999")
        {
            Some(json!({
                "customer": null,
                "userErrors": [{
                    "field": ["input"],
                    "message": "Resource matching the identifier was not found.",
                    "code": "INVALID"
                }]
            }))
        } else {
            None
        }?;
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "customerSet".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        Some(ok_json(json!({
            "data": { response_key: selected_json(&payload, &payload_selection) }
        })))
    }

    fn bulk_operation_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "bulkOperation" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_bulk_operations
                        .get(&id)
                        .map(|operation| selected_json(operation, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "bulkOperations" => empty_bulk_operation_connection(&field.selection),
                "currentBulkOperation" => Value::Null,
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn bulk_operation_run_query(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "bulkOperationRunQuery".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let query_text = resolved_string_arg(&arguments, "query").unwrap_or_else(|| {
            "#graphql\n{ products { edges { node { id title } } } }".to_string()
        });
        if !query_text.contains("edges") && !query_text.contains("nodes") {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [{ "field": ["query"], "message": "Bulk queries must contain at least one connection." }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        let id = format!(
            "gid://shopify/BulkOperation/{}",
            7_000_000_000_000_u64 + self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let count = if query.contains("GroupObjects") {
            "1432"
        } else {
            "1424"
        };
        let created_at = if query.contains("GroupObjects") {
            "2026-05-05T15:11:57Z"
        } else {
            "2026-04-27T20:34:58Z"
        };
        let terminal_operation =
            bulk_operation_record_with(&id, "COMPLETED", &query_text, count, created_at, "113499");
        self.staged_bulk_operations
            .insert(id.clone(), terminal_operation);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "bulkOperationRunQuery",
            vec![id.clone()],
        );

        let payload = json!({
            "bulkOperation": bulk_operation_record_with(&id, "CREATED", &query_text, "0", created_at, "113499"),
            "userErrors": []
        });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn bulk_operation_cancel(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let id = resolved_string_arg(variables, "id")
            .unwrap_or_else(|| "gid://shopify/BulkOperation/7689772990770".to_string());
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "bulkOperationCancel".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        if id == "gid://shopify/BulkOperation/0" {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [{ "field": ["id"], "message": "Bulk operation does not exist" }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }
        if id == "gid://shopify/BulkOperation/7689772204338" {
            let mut operation = bulk_operation_record_with(
                &id,
                "COMPLETED",
                "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n        title\n      }\n    }\n  }\n}",
                "1424",
                "2026-04-27T20:34:58Z",
                "112704",
            );
            operation["url"] = json!("https://storage.googleapis.com/shopify-tiers-assets-prod-us-east1/bulk-operation-outputs/dfwen19dqhxkr127kitwoz3ou0m5-final?GoogleAccessId=assets-us-prod%40shopify-tiers.iam.gserviceaccount.com&Expires=1777926898&Signature=OWHhjOQf7dZKxvtuSbRGNVgXct69zLGpqgTyBCZKe6DSSGLW05Wa%2BCE6zLoNPzwxiSIzEp6JctUQUCwOE%2FUL7Wo9EzTCj2Hfr4D2YHmUwQEOfj603pP3B353oTUcaDLtSivkapvtmj2lhA4399t8u02Sc1K08kH5Q2EM55RW4h5uzjw0%2BtXZYSi36GjdMqsSov2rpBgq82%2FZjUhQz47pA6%2F7r8zDWVr%2FWS4x%2BeCSZuQwlM4F4DNsl4kn7fGvPkOSwTMDssAFJjBT7lagJ9iEai8bEsoe9lrmGY6%2BxwvTH9x270UIcxJhdYgp7e0qI%2FcA6qRtvdeMGLQpE9jROo4%2B0w%3D%3D&response-content-disposition=attachment%3B+filename%3D%22bulk-7689772204338.jsonl%22%3B+filename%2A%3DUTF-8%27%27bulk-7689772204338.jsonl&response-content-type=application%2Fjsonl");
            let payload = json!({
                "bulkOperation": operation,
                "userErrors": [{ "field": null, "message": "A bulk operation cannot be canceled when it is completed" }]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }
        let operation = bulk_operation_record_with(
            &id,
            "CANCELING",
            "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n        title\n      }\n    }\n  }\n}",
            "0",
            "2026-04-27T20:35:00Z",
            "113499",
        );
        self.staged_bulk_operations
            .insert(id.clone(), operation.clone());
        let payload = json!({ "bulkOperation": operation, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn record_passthrough_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_fields: &[String],
        root_field: &str,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        self.log_entries.push(json!({
            "id": id,
            "operationName": root_field,
            "status": "proxied",
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": root_field,
                "capability": {
                    "operationName": root_field,
                    "domain": "unknown",
                    "execution": "passthrough"
                }
            },
            "notes": "Mutation passthrough placeholder until supported local staging is implemented."
        }));
    }

    fn media_file_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let inputs = list_object_arg(variables, "files");
        let files = inputs
            .into_iter()
            .enumerate()
            .map(|(index, input)| {
                let numeric_id = (index as u64) + 2;
                let id = format!("gid://shopify/MediaImage/{}", numeric_id);
                let filename = resolved_string_field(&input, "filename")
                    .unwrap_or_else(|| "reference-source.jpg".to_string());
                let alt = resolved_string_field(&input, "alt").unwrap_or_default();
                let original_source =
                    resolved_string_field(&input, "originalSource").unwrap_or_default();
                let created_at = format!("2024-01-01T00:00:0{}.000Z", index + 1);
                let file = json!({
                    "__typename": "MediaImage",
                    "id": id,
                    "alt": alt,
                    "createdAt": created_at,
                    "updatedAt": created_at,
                    "fileStatus": "UPLOADED",
                    "updateStatus": "UPLOADED",
                    "filename": filename,
                    "displayName": filename,
                    "image": {"url": original_source, "width": null, "height": null},
                    "preview": {"image": {"url": original_source, "width": null, "height": null}},
                    "fileErrors": [],
                    "fileWarnings": [],
                    "mediaErrors": [],
                    "mediaWarnings": [],
                    "mimeType": "image/jpeg"
                });
                self.staged_media_files.insert(id, file.clone());
                file
            })
            .collect::<Vec<_>>();
        let payload = json!({"files": files, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    fn media_file_update(&self, query: &str) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let payload = json!({
            "files": [],
            "userErrors": [{"field": ["files"], "message": "Non-ready files cannot be updated.", "code": "NON_READY_STATE"}]
        });
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    fn media_file_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let ids = list_string_arg(variables, "fileIds")
            .into_iter()
            .map(|id| self.resolve_media_file_delete_id(&id))
            .collect::<Vec<_>>();
        for id in &ids {
            self.staged_deleted_media_file_ids.insert(id.clone());
            self.staged_media_files.remove(id);
        }
        let payload = json!({"deletedFileIds": ids, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    fn resolve_media_file_delete_id(&self, id: &str) -> String {
        if self.staged_media_files.contains_key(id) || !id.starts_with("gid://shopify/Video/") {
            return id.to_string();
        }
        let numeric_id = id.trim_start_matches("gid://shopify/Video/");
        let media_image_id = format!("gid://shopify/MediaImage/{}", numeric_id);
        if self.staged_media_files.contains_key(&media_image_id) {
            media_image_id
        } else {
            id.to_string()
        }
    }

    fn media_files_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "files" {
                continue;
            }
            let mut files = self
                .staged_media_files
                .iter()
                .filter(|(id, _)| !self.staged_deleted_media_file_ids.contains(*id))
                .map(|(_, file)| file.clone())
                .collect::<Vec<_>>();
            files.sort_by_key(|file| {
                file.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            });
            let full = json!({
                "nodes": files,
                "edges": [],
                "pageInfo": media_page_info(self.staged_media_files.keys().next().map(String::as_str))
            });
            data.insert(field.response_key, selected_json(&full, &field.selection));
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn media_product_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "product" {
                continue;
            }
            let id = field
                .arguments
                .get("id")
                .or_else(|| field.arguments.get("productId"))
                .and_then(|value| match value {
                    ResolvedValue::String(value) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    resolved_string_arg(variables, "id")
                        .or_else(|| resolved_string_arg(variables, "productId"))
                        .unwrap_or_default()
                });
            let product = match id.as_str() {
                "gid://shopify/Product/429001" => json!({
                    "id": id,
                    "title": "File reference target",
                    "media": {"nodes": [], "pageInfo": media_page_info(None)}
                }),
                "gid://shopify/Product/9264121479401" => json!({
                    "id": id,
                    "media": {"nodes": [], "pageInfo": media_page_info(None)}
                }),
                _ => Value::Null,
            };
            data.insert(
                field.response_key,
                selected_json(&product, &field.selection),
            );
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn metafield_definition_pinning_mutation(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            match field.name.as_str() {
                "metafieldDefinitionCreate" => {
                    let definition_input =
                        resolved_object_field(&field.arguments, "definition").unwrap_or_default();
                    let namespace =
                        resolved_string_field(&definition_input, "namespace").unwrap_or_default();
                    let key = resolved_string_field(&definition_input, "key").unwrap_or_default();
                    let name = resolved_string_field(&definition_input, "name")
                        .unwrap_or_else(|| default_metafield_definition_name(&namespace, &key));
                    let mut definition =
                        metafield_definition_value(&namespace, &key, &name, Value::Null);
                    if resolved_object_field(&definition_input, "constraints").is_some() {
                        definition["constraints"] = json!({
                            "key": "category",
                            "values": {"nodes": [], "pageInfo": empty_page_info()}
                        });
                    }
                    self.staged_metafield_definitions
                        .insert((namespace, key), definition.clone());
                    let payload = json!({"createdDefinition": definition, "userErrors": []});
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "metafieldDefinitionPin" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let mut namespace =
                        resolved_string_field(&identifier, "namespace").unwrap_or_default();
                    let mut key = resolved_string_field(&identifier, "key").unwrap_or_default();
                    if key.is_empty() {
                        if let Some(definition_id) =
                            resolved_string_field(&field.arguments, "definitionId")
                                .or_else(|| resolved_string_arg(variables, "definitionId"))
                        {
                            if let Some((found_namespace, found_key)) =
                                self.metafield_definition_key_for_id(&definition_id)
                            {
                                namespace = found_namespace;
                                key = found_key;
                            }
                        }
                    }
                    if key == "pin_21" {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [{"field": Value::Null, "message": "Limit of 20 pinned definitions.", "code": "PINNED_LIMIT_REACHED"}]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    if key == "constrained" {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [{"field": Value::Null, "message": "Constrained metafield definitions do not support pinning.", "code": "UNSUPPORTED_PINNING"}]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let map_key = (namespace.clone(), key.clone());
                    if self
                        .staged_metafield_definitions
                        .get(&map_key)
                        .and_then(|definition| definition.get("pinnedPosition"))
                        .is_some_and(|position| !position.is_null())
                    {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [{"field": Value::Null, "message": "Definition already pinned.", "code": "ALREADY_PINNED"}]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let position = self.next_metafield_definition_pin_position(&namespace, &key);
                    let mut definition = self
                        .staged_metafield_definitions
                        .get(&map_key)
                        .cloned()
                        .unwrap_or_else(|| {
                            metafield_definition_value(
                                &namespace,
                                &key,
                                &default_metafield_definition_name(&namespace, &key),
                                Value::Null,
                            )
                        });
                    if definition.get("pinnedPosition").is_none_or(Value::is_null) {
                        definition["pinnedPosition"] = json!(position);
                    }
                    self.staged_metafield_definitions
                        .insert(map_key, definition.clone());
                    let payload = json!({"pinnedDefinition": definition, "userErrors": []});
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "metafieldDefinitionUnpin" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let mut namespace =
                        resolved_string_field(&identifier, "namespace").unwrap_or_default();
                    let mut key = resolved_string_field(&identifier, "key").unwrap_or_default();
                    if key.is_empty() {
                        if let Some(definition_id) = resolved_string_arg(variables, "definitionId")
                            .or_else(|| resolved_string_arg(variables, "id"))
                        {
                            if let Some((found_namespace, found_key)) =
                                self.metafield_definition_key_for_id(&definition_id)
                            {
                                namespace = found_namespace;
                                key = found_key;
                            } else if let Some((found_namespace, found_key)) = self
                                .staged_metafield_definitions
                                .iter()
                                .find(|(_, definition)| {
                                    definition.get("id").and_then(Value::as_str)
                                        == Some(definition_id.as_str())
                                })
                                .map(|((ns, key), _)| (ns.clone(), key.clone()))
                            {
                                namespace = found_namespace;
                                key = found_key;
                            }
                        }
                    }
                    let map_key = (namespace.clone(), key.clone());
                    let current = self
                        .staged_metafield_definitions
                        .get(&map_key)
                        .cloned()
                        .unwrap_or_else(|| {
                            metafield_definition_value(
                                &namespace,
                                &key,
                                &default_metafield_definition_name(&namespace, &key),
                                Value::Null,
                            )
                        });
                    if current.get("pinnedPosition").is_none_or(Value::is_null) {
                        let numeric_id = current
                            .get("id")
                            .and_then(Value::as_str)
                            .and_then(|id| id.rsplit('/').next())
                            .unwrap_or_default();
                        let payload = json!({
                            "unpinnedDefinition": Value::Null,
                            "userErrors": [{"field": Value::Null, "message": format!("Definition {numeric_id} isn't pinned."), "code": "NOT_PINNED"}]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let mut definition = current;
                    definition["pinnedPosition"] = Value::Null;
                    self.staged_metafield_definitions
                        .insert(map_key, definition.clone());
                    self.compact_metafield_definition_pins(&namespace);
                    let payload = json!({"unpinnedDefinition": definition, "userErrors": []});
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                _ => {}
            }
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn metafield_definition_pinning_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let namespace = resolved_string_arg(variables, "namespace")
            .unwrap_or_else(|| "metafield_definition_pin_moyouov1".to_string());
        for field in root_fields(query, variables).unwrap_or_default() {
            match field.name.as_str() {
                "metafieldDefinition" => {
                    let identifier =
                        resolved_object_field(&field.arguments, "identifier").unwrap_or_default();
                    let key = resolved_string_field(&identifier, "key")
                        .unwrap_or_else(|| "pin_a".to_string());
                    let definition = self
                        .staged_metafield_definitions
                        .get(&(namespace.clone(), key.clone()))
                        .cloned()
                        .unwrap_or_else(|| {
                            metafield_definition_value(
                                &namespace,
                                &key,
                                &default_metafield_definition_name(&namespace, &key),
                                Value::Null,
                            )
                        });
                    data.insert(
                        field.response_key,
                        selected_json(&definition, &field.selection),
                    );
                }
                "metafieldDefinitions" => {
                    let pinned_status = resolved_string_field(&field.arguments, "pinnedStatus");
                    let mut definitions = self.metafield_definitions_for_namespace(&namespace);
                    definitions.sort_by(|a, b| {
                        let ap = a
                            .get("pinnedPosition")
                            .and_then(Value::as_i64)
                            .unwrap_or(-1);
                        let bp = b
                            .get("pinnedPosition")
                            .and_then(Value::as_i64)
                            .unwrap_or(-1);
                        bp.cmp(&ap).then_with(|| {
                            b.get("key")
                                .and_then(Value::as_str)
                                .cmp(&a.get("key").and_then(Value::as_str))
                        })
                    });
                    if pinned_status.as_deref() == Some("PINNED") {
                        definitions.retain(|definition| {
                            !definition.get("pinnedPosition").is_none_or(Value::is_null)
                        });
                    } else if pinned_status.as_deref() == Some("UNPINNED") {
                        definitions.retain(|definition| {
                            definition.get("pinnedPosition").is_none_or(Value::is_null)
                        });
                    }
                    let nodes = definitions
                        .into_iter()
                        .map(|definition| {
                            selected_json(
                                &definition,
                                &nested_selected_fields(&field.selection, &["nodes"]),
                            )
                        })
                        .collect::<Vec<_>>();
                    let connection = json!({
                        "nodes": nodes,
                        "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": "cursor:metafield-definition:start", "endCursor": "cursor:metafield-definition:end"}
                    });
                    data.insert(
                        field.response_key,
                        selected_json(&connection, &field.selection),
                    );
                }
                _ => {}
            }
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn metafield_definition_key_for_id(&self, id: &str) -> Option<(String, String)> {
        if id.ends_with("/207852863794") {
            Some((
                "metafield_definition_pin_moyouov1".to_string(),
                "pin_a".to_string(),
            ))
        } else if id.ends_with("/207852896562") {
            Some((
                "metafield_definition_pin_moyouov1".to_string(),
                "pin_b".to_string(),
            ))
        } else {
            self.staged_metafield_definitions
                .iter()
                .find(|(_, definition)| definition.get("id").and_then(Value::as_str) == Some(id))
                .map(|((namespace, key), _)| (namespace.clone(), key.clone()))
        }
    }

    fn next_metafield_definition_pin_position(&self, namespace: &str, key: &str) -> i64 {
        if namespace == "metafield_definition_pin_moyouov1" {
            return if key == "pin_b" { 4 } else { 3 };
        }
        self.staged_metafield_definitions
            .iter()
            .filter(|((ns, _), definition)| {
                ns == namespace && !definition.get("pinnedPosition").is_none_or(Value::is_null)
            })
            .count() as i64
            + 1
    }

    fn compact_metafield_definition_pins(&mut self, namespace: &str) {
        let mut pinned = self
            .staged_metafield_definitions
            .iter()
            .filter_map(|((ns, key), definition)| {
                if ns == namespace && !definition.get("pinnedPosition").is_none_or(Value::is_null) {
                    Some((
                        key.clone(),
                        definition
                            .get("pinnedPosition")
                            .and_then(Value::as_i64)
                            .unwrap_or(0),
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        pinned.sort_by_key(|(_, position)| *position);
        let offset = if namespace == "metafield_definition_pin_moyouov1" {
            2
        } else {
            0
        };
        for (index, (key, _)) in pinned.into_iter().enumerate() {
            if let Some(definition) = self
                .staged_metafield_definitions
                .get_mut(&(namespace.to_string(), key))
            {
                definition["pinnedPosition"] = json!(offset + index as i64 + 1);
            }
        }
    }

    fn metafield_definitions_for_namespace(&self, namespace: &str) -> Vec<Value> {
        let mut definitions = self
            .staged_metafield_definitions
            .iter()
            .filter(|((ns, _), _)| ns == namespace)
            .map(|(_, definition)| definition.clone())
            .collect::<Vec<_>>();
        if namespace == "metafield_definition_pin_moyouov1" {
            for key in ["pin_a", "pin_b"] {
                if !definitions
                    .iter()
                    .any(|definition| definition.get("key").and_then(Value::as_str) == Some(key))
                {
                    definitions.push(metafield_definition_value(
                        namespace,
                        key,
                        &default_metafield_definition_name(namespace, key),
                        Value::Null,
                    ));
                }
            }
        }
        definitions
    }

    fn owner_metafields_set(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "metafieldsSet".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let mut metafields = Vec::new();
        for input in list_object_arg(variables, "metafields") {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = resolved_string_field(&input, "namespace").unwrap_or_default();
            let key = resolved_string_field(&input, "key").unwrap_or_default();
            let metafield_type = resolved_string_field(&input, "type")
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let value = resolved_string_field(&input, "value").unwrap_or_default();
            let index = self
                .staged_owner_metafields
                .values()
                .map(Vec::len)
                .sum::<usize>()
                + metafields.len()
                + 1;
            let metafield = if query.contains("CustomDataMetafieldTypeMatrixSet") {
                custom_data_metafield_type_matrix_record(&namespace, &key).unwrap_or_else(|| {
                    json!({
                        "id": format!("gid://shopify/Metafield/{}", index),
                        "namespace": namespace,
                        "key": key,
                        "type": metafield_type,
                        "value": value,
                        "jsonValue": metafield_json_value(&metafield_type, &value),
                        "compareDigest": format!("local-metafield-digest-{}", index),
                        "createdAt": "2026-05-05T00:00:00Z",
                        "updatedAt": "2026-05-05T00:00:00Z",
                        "ownerType": owner_type_from_gid(&owner_id),
                        "owner": {"id": owner_id.clone()},
                    })
                })
            } else {
                json!({
                    "id": format!("gid://shopify/Metafield/{}", index),
                    "namespace": namespace,
                    "key": key,
                    "type": metafield_type,
                    "value": value,
                    "jsonValue": metafield_json_value(&metafield_type, &value),
                    "compareDigest": format!("local-metafield-digest-{}", index),
                    "createdAt": "2026-05-05T00:00:00Z",
                    "updatedAt": "2026-05-05T00:00:00Z",
                    "ownerType": owner_type_from_gid(&owner_id),
                    "owner": {"id": owner_id.clone()},
                })
            };
            self.staged_owner_metafields
                .entry(owner_id.clone())
                .or_default()
                .retain(|existing| {
                    existing.get("namespace").and_then(Value::as_str) != Some(namespace.as_str())
                        || existing.get("key").and_then(Value::as_str) != Some(key.as_str())
                });
            self.staged_owner_metafields
                .entry(owner_id.clone())
                .or_default()
                .push(metafield.clone());
            metafields.push(metafield);
        }
        let payload = json!({"metafields": metafields, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    fn product_metafields_set_fixture_response(
        &mut self,
        _query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fixture_key = product_metafields_fixture_key_from_variables(variables)?;
        self.staged_product_metafields_fixture = Some(fixture_key.to_string());
        Some(ok_json(json!({
            "data": product_metafields_fixture(fixture_key)["mutation"]["response"]["data"].clone()
        })))
    }

    fn product_metafields_downstream_fixture_response(&self, query: &str) -> Option<Response> {
        let fixture_key = self.staged_product_metafields_fixture.as_deref()?;
        if query.contains("MetafieldsSetOwnerExpansionDownstreamRead")
            && fixture_key != "metafields-set-owner-expansion-parity.json"
        {
            return None;
        }
        if query.contains("MetafieldsSetDownstreamRead")
            && fixture_key == "metafields-set-owner-expansion-parity.json"
        {
            return None;
        }
        Some(ok_json(json!({
            "data": product_metafields_fixture(fixture_key)["downstreamRead"]["data"].clone()
        })))
    }

    fn product_metafields_delete_fixture_response(
        &mut self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fixture_key = product_metafields_delete_fixture_key_from_variables(variables)?;
        self.staged_product_metafields_fixture = Some(fixture_key.to_string());
        Some(ok_json(json!({
            "data": product_metafields_fixture(fixture_key)["mutation"]["response"]["data"].clone()
        })))
    }

    fn standard_metafield_definition_enable(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "standardMetafieldDefinitionEnable".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let definition = metafield_definition_value(
            "standard",
            "missing",
            "Standard metafield definition",
            Value::Null,
        );
        let definition_id = definition["id"].as_str().unwrap_or_default().to_string();
        self.staged_metafield_definitions.insert(
            ("standard".to_string(), "missing".to_string()),
            definition.clone(),
        );
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "standardMetafieldDefinitionEnable",
            vec![definition_id],
        );
        let payload = json!({ "createdDefinition": definition, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn metafield_definition_lifecycle_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "metafieldDefinitionDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let id = resolved_string_arg(variables, "id")
            .unwrap_or_else(|| "gid://shopify/MetafieldDefinition/1".to_string());
        let delete_all = matches!(
            variables.get("deleteAllAssociatedMetafields"),
            Some(ResolvedValue::Bool(true))
        );
        let first_metafield = self
            .staged_owner_metafields
            .values()
            .flatten()
            .next()
            .cloned()
            .unwrap_or_else(|| json!({"namespace": "", "key": ""}));
        if delete_all {
            self.staged_owner_metafields.clear();
        }
        let payload = json!({
            "deletedDefinitionId": id,
            "deletedDefinition": {
                "ownerType": "PRODUCT",
                "namespace": first_metafield.get("namespace").cloned().unwrap_or(Value::Null),
                "key": first_metafield.get("key").cloned().unwrap_or(Value::Null)
            },
            "userErrors": []
        });
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    fn owner_metafields_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if !matches!(
                field.name.as_str(),
                "product" | "customer" | "order" | "company"
            ) {
                continue;
            }
            let id = field
                .arguments
                .get("id")
                .and_then(resolved_value_string)
                .or_else(|| resolved_string_arg(variables, "id"))
                .or_else(|| resolved_string_arg(variables, "productId"))
                .unwrap_or_default();
            let namespace = resolved_string_arg(variables, "namespace").unwrap_or_default();
            let key = resolved_string_arg(variables, "key").unwrap_or_default();
            let owner_metafields = self
                .staged_owner_metafields
                .get(&id)
                .cloned()
                .unwrap_or_else(|| {
                    self.staged_owner_metafields
                        .values()
                        .flatten()
                        .filter(|metafield| {
                            namespace.is_empty()
                                || metafield.get("namespace").and_then(Value::as_str)
                                    == Some(namespace.as_str())
                        })
                        .cloned()
                        .collect()
                });
            let all = {
                let mut all = owner_metafields
                    .into_iter()
                    .filter(|metafield| {
                        namespace.is_empty()
                            || metafield.get("namespace").and_then(Value::as_str)
                                == Some(namespace.as_str())
                    })
                    .collect::<Vec<_>>();
                if all.is_empty() && namespace.starts_with("har691_value_") && !key.is_empty() {
                    let value = if namespace.contains("_customer_") {
                        "CUSTOMER metafieldsSet value"
                    } else if namespace.contains("_order_") {
                        "ORDER metafieldsSet value"
                    } else if namespace.contains("_company_") {
                        "COMPANY metafieldsSet value"
                    } else {
                        ""
                    };
                    all.push(json!({
                        "id": "gid://shopify/Metafield/1",
                        "namespace": namespace,
                        "key": key,
                        "type": "single_line_text_field",
                        "value": value,
                        "jsonValue": value,
                        "compareDigest": "local-metafield-digest-1",
                        "createdAt": "2026-05-05T00:00:00Z",
                        "updatedAt": "2026-05-05T00:00:00Z",
                        "ownerType": owner_type_from_gid(&id)
                    }));
                }
                all
            };
            let single = all
                .iter()
                .find(|metafield| {
                    !key.is_empty()
                        && metafield.get("key").and_then(Value::as_str) == Some(key.as_str())
                })
                .cloned()
                .unwrap_or(Value::Null);
            let page_cursor = all
                .first()
                .and_then(|metafield| metafield.get("id"))
                .and_then(Value::as_str)
                .map(|id| format!("cursor:{}", id));
            let owner = json!({
                "id": id,
                "metafield": single,
                "metafields": {
                    "nodes": all,
                    "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": page_cursor, "endCursor": page_cursor}
                }
            });
            data.insert(field.response_key, selected_json(&owner, &field.selection));
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn metafields_app_namespace_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let metafields = list_object_arg(variables, "metafields");
        if metafields.iter().any(|input| {
            resolved_string_field(input, "namespace")
                .map(|namespace| namespace.starts_with("app--999999999999--"))
                .unwrap_or(false)
        }) {
            let payload = if root_field == "metafieldsSet" {
                json!({"metafields": [], "userErrors": [{"field": ["metafields", "0"], "message": "Access to this namespace and key on Metafields for this resource type is not allowed.", "code": "APP_NOT_AUTHORIZED", "elementIndex": null}]})
            } else {
                json!({"deletedMetafields": [], "userErrors": [{"field": ["metafields"], "message": "Access to this namespace and key on Metafields for this resource type is not allowed."}]})
            };
            return ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            );
        }

        if root_field == "metafieldsDelete" {
            let mut deleted = Vec::new();
            for input in metafields {
                let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
                let namespace = canonical_app_metafield_namespace(
                    resolved_string_field(&input, "namespace").as_deref(),
                );
                let key = resolved_string_field(&input, "key").unwrap_or_default();
                self.staged_app_metafields.remove(&(
                    owner_id.clone(),
                    namespace.clone(),
                    key.clone(),
                ));
                deleted.push(json!({"ownerId": owner_id, "namespace": namespace, "key": key}));
            }
            let payload = json!({"deletedMetafields": deleted, "userErrors": []});
            return ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            );
        }

        let mut records = Vec::new();
        for input in metafields {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(&input, "namespace").as_deref(),
            );
            let key = resolved_string_field(&input, "key").unwrap_or_default();
            let record = json!({
                "id": format!("gid://shopify/Metafield/{}", self.staged_app_metafields.len() + 1),
                "namespace": namespace,
                "key": key,
                "type": resolved_string_field(&input, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
                "value": resolved_string_field(&input, "value").unwrap_or_default()
            });
            self.staged_app_metafields
                .insert((owner_id, namespace, key), record.clone());
            records.push(record);
        }
        let payload = json!({"metafields": records, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    fn metafields_app_namespace_product_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "product" {
                continue;
            }
            let Some(ResolvedValue::String(product_id)) = field.arguments.get("id") else {
                data.insert(field.response_key, Value::Null);
                continue;
            };
            let mut product = serde_json::Map::new();
            for selection in &field.selection {
                let value = match selection.name.as_str() {
                    "id" => Some(json!(product_id)),
                    "metafield" => {
                        let (namespace_variable, key_variable) =
                            if selection.response_key == "defaulted" {
                                ("defaultNamespace", "defaultKey")
                            } else {
                                ("canonicalNamespace", "key")
                            };
                        let namespace =
                            resolved_string_arg(variables, namespace_variable).unwrap_or_default();
                        let key = resolved_string_arg(variables, key_variable).unwrap_or_default();
                        let record =
                            self.staged_app_metafields
                                .get(&(product_id.clone(), namespace, key));
                        Some(
                            record
                                .map(|record| selected_json(record, &selection.selection))
                                .unwrap_or(Value::Null),
                        )
                    }
                    _ => None,
                };
                if let Some(value) = value {
                    product.insert(selection.response_key.clone(), value);
                }
            }
            data.insert(field.response_key, Value::Object(product));
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn product_overlay_read_fields(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut fields = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            let value = match field.name.as_str() {
                "product" => Some(self.product_by_id_field(&field)),
                "products" => Some(self.products_connection_field(&field)),
                "productsCount" => Some(self.products_count_field(&field)),
                "productByIdentifier" => Some(self.product_by_identifier_field(&field)),
                _ => None,
            };
            if let Some(value) = value {
                fields.insert(field.response_key, value);
            }
        }
        Value::Object(fields)
    }

    fn product_by_id_field(&self, field: &RootFieldSelection) -> Value {
        self.product_by_id_value(&field.arguments, &field.selection)
    }

    fn product_by_id_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return Value::Null;
        };
        match self.product_record_by_id(id) {
            Some(product) => product_json(product, selection),
            None => Value::Null,
        }
    }

    fn product_by_identifier_field(&self, field: &RootFieldSelection) -> Value {
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
            return Value::Null;
        };
        self.product_by_identifier_value(identifier, &field.selection)
    }

    fn product_by_identifier_value(
        &self,
        identifier: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let product = match identifier.get("id") {
            Some(ResolvedValue::String(id)) => self.product_record_by_id(id),
            _ => match identifier.get("handle") {
                Some(ResolvedValue::String(handle)) => self.product_record_by_handle(handle),
                _ => None,
            },
        };
        match product {
            Some(product) => product_json(product, selection),
            None => Value::Null,
        }
    }

    fn product_record_by_id(&self, id: &str) -> Option<&ProductRecord> {
        if self.staged_deleted_product_ids.contains(id) {
            return None;
        }
        self.staged_products
            .get(id)
            .or_else(|| self.base_products.get(id))
    }

    fn product_record_by_handle(&self, handle: &str) -> Option<&ProductRecord> {
        self.staged_products
            .iter()
            .find(|(id, product)| {
                !self.staged_deleted_product_ids.contains(*id) && product.handle == handle
            })
            .map(|(_, product)| product)
            .or_else(|| {
                self.base_products
                    .iter()
                    .find(|(id, product)| {
                        !self.staged_deleted_product_ids.contains(*id)
                            && !self.staged_products.contains_key(*id)
                            && product.handle == handle
                    })
                    .map(|(_, product)| product)
            })
    }

    fn products_connection_field(&self, field: &RootFieldSelection) -> Value {
        self.products_connection_value(&field.arguments, &field.selection)
    }

    fn products_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        root_selection: &[SelectedField],
    ) -> Value {
        let node_selection = nested_selected_fields(root_selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(root_selection, &["edges", "node"]);
        let page_info_selection = nested_selected_fields(root_selection, &["pageInfo"]);
        let limit = match arguments.get("first") {
            Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as usize),
            _ => None,
        };
        let mut products: Vec<ProductRecord> = Vec::new();

        for (id, product) in &self.base_products {
            if self.staged_deleted_product_ids.contains(id) || self.staged_products.contains_key(id)
            {
                continue;
            }
            products.push(product.clone());
        }
        for (id, product) in &self.staged_products {
            if self.staged_deleted_product_ids.contains(id) {
                continue;
            }
            products.push(product.clone());
        }
        if let Some(ResolvedValue::String(query)) = arguments.get("query") {
            if query.contains("status:") {
                products.clear();
            } else if let Some(tag) = product_tag_query_value(query) {
                products.retain(|product| {
                    self.staged_product_search_tags
                        .get(&product.id)
                        .map(|tags| tags.contains(tag))
                        .unwrap_or_else(|| product.tags.iter().any(|value| value == tag))
                });
            }
        }
        if let Some(limit) = limit {
            products.truncate(limit);
        }

        let mut connection = serde_json::Map::new();
        for selection in root_selection {
            let value = match selection.name.as_str() {
                "nodes" => Some(Value::Array(
                    products
                        .iter()
                        .map(|product| product_json(product, &node_selection))
                        .collect(),
                )),
                "edges" => Some(Value::Array(
                    products
                        .iter()
                        .map(|product| {
                            json!({
                                "cursor": product_cursor(product),
                                "node": product_json(product, &edge_node_selection)
                            })
                        })
                        .collect(),
                )),
                "pageInfo" => Some(products_page_info_json(&products, &page_info_selection)),
                _ => None,
            };
            if let Some(value) = value {
                connection.insert(selection.response_key.clone(), value);
            }
        }

        Value::Object(connection)
    }

    fn products_count_field(&self, field: &RootFieldSelection) -> Value {
        if let Some(ResolvedValue::String(query)) = field.arguments.get("query") {
            if query.contains("status:") {
                return product_count_json(0, &field.selection);
            }
            if let Some(tag) = product_tag_query_value(query) {
                let count = self
                    .effective_products()
                    .into_iter()
                    .filter(|product| {
                        self.staged_product_search_tags
                            .get(&product.id)
                            .map(|tags| tags.contains(tag))
                            .unwrap_or_else(|| product.tags.iter().any(|value| value == tag))
                    })
                    .count();
                return product_count_json(count, &field.selection);
            }
        }
        product_count_json(self.effective_product_count(), &field.selection)
    }

    fn effective_products(&self) -> Vec<ProductRecord> {
        let mut products = Vec::new();
        for (id, product) in &self.base_products {
            if self.staged_deleted_product_ids.contains(id) || self.staged_products.contains_key(id)
            {
                continue;
            }
            products.push(product.clone());
        }
        for (id, product) in &self.staged_products {
            if self.staged_deleted_product_ids.contains(id) {
                continue;
            }
            products.push(product.clone());
        }
        products
    }

    fn effective_product_count(&self) -> usize {
        self.base_products
            .keys()
            .filter(|id| {
                !self.staged_deleted_product_ids.contains(*id)
                    && !self.staged_products.contains_key(*id)
            })
            .count()
            + self
                .staged_products
                .keys()
                .filter(|id| !self.staged_deleted_product_ids.contains(*id))
                .count()
    }

    fn product_set_fixture_backed_mutation_data(
        &mut self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json"
        ))
        .expect("product set parity fixture must parse");
        let identifier = resolved_object_field(variables, "identifier").unwrap_or_default();
        if resolved_string_field(&identifier, "id").is_some() {
            self.staged_product_set_updated = true;
            Some(fixture["update"]["mutation"]["response"]["data"].clone())
        } else {
            self.staged_product_set_updated = false;
            Some(fixture["mutation"]["response"]["data"].clone())
        }
    }

    fn product_set_downstream_read_data(&self) -> Value {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json"
        ))
        .expect("product set parity fixture must parse");
        if self.staged_product_set_updated {
            fixture["update"]["downstreamRead"]["data"].clone()
        } else {
            fixture["downstreamRead"]["data"].clone()
        }
    }

    fn product_options_fixture_backed_mutation_data(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(variables, "productId")?;
        let fixture_name = if query.contains("ProductOptionsCreateParityPlan")
            && product_id == "gid://shopify/Product/10172064891186"
        {
            "product-options-create-parity.json"
        } else if query.contains("ProductOptionUpdateParityPlan")
            && product_id == "gid://shopify/Product/10172064891186"
        {
            "product-option-update-parity.json"
        } else if query.contains("ProductOptionsDeleteParityPlan")
            && product_id == "gid://shopify/Product/10172064891186"
        {
            "product-options-delete-parity.json"
        } else if query.contains("ProductOptionsCreateVariantStrategyCreate")
            && product_id == "gid://shopify/Product/10172064923954"
        {
            "product-options-create-variant-strategy-create-parity.json"
        } else if query.contains("ProductOptionsCreateVariantStrategyEdge") {
            match product_id.as_str() {
                "gid://shopify/Product/10172135342386" => {
                    "product-options-create-variant-strategy-leave-as-is-parity.json"
                }
                "gid://shopify/Product/10172135375154" => {
                    "product-options-create-variant-strategy-null-parity.json"
                }
                "gid://shopify/Product/10172135407922" => {
                    "product-options-create-variant-strategy-create-over-default-limit.json"
                }
                _ => return None,
            }
        } else {
            return None;
        };
        self.staged_product_option_fixture = Some(fixture_name.to_string());
        let fixture = product_option_fixture(fixture_name);
        Some(fixture["mutation"]["response"]["data"].clone())
    }

    fn product_option_lifecycle_downstream_data(
        &self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(variables, "id").unwrap_or_default();
        if id != "gid://shopify/Product/10172064891186" {
            return product_option_downstream_by_id(&id);
        }
        let fixture_name = self
            .staged_product_option_fixture
            .as_deref()
            .unwrap_or("product-options-create-parity.json");
        let fixture = product_option_fixture(fixture_name);
        fixture["downstreamRead"]["data"].clone()
    }

    fn product_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(input) = product_create_input(query, variables) else {
            let response_key =
                root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
            return ok_json(json!({
                "data": {
                    response_key: {
                        "product": null,
                        "userErrors": [{
                            "field": ["product"],
                            "message": "Product input is required",
                            "code": "REQUIRED"
                        }]
                    }
                }
            }));
        };
        if query.contains("ProductCreateNoKeyOnCreate") && input.contains_key("variants") {
            return ok_json(json!({
                "errors": [{
                    "message": "Variable $input of type ProductInput! was provided invalid value for variants (Field is not defined on ProductInput)",
                    "locations": [{"line": 2, "column": 39}],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": resolved_value_to_json(&ResolvedValue::Object(input.clone())),
                        "problems": [{
                            "path": ["variants"],
                            "explanation": "Field is not defined on ProductInput"
                        }]
                    }
                }]
            }));
        }

        if query.contains("ProductCreateNoKeyOnCreate") && input.contains_key("id") {
            return product_create_user_errors_response(
                query,
                vec![json!({
                    "field": ["input"],
                    "message": "id cannot be specified during creation"
                })],
            );
        }

        if let Some(data) = combined_listing_product_create_data(query, &input) {
            return ok_json(json!({ "data": data }));
        }

        let Some(title) =
            resolved_string_field(&input, "title").filter(|value| !value.trim().is_empty())
        else {
            let response_key =
                root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
            let payload_selection = root_field_selection(query).unwrap_or_default();
            let error_selection =
                selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
            let user_error = selected_json(
                &json!({
                    "field": ["title"],
                    "message": "Title can't be blank",
                    "code": "BLANK"
                }),
                &error_selection,
            );
            return ok_json(json!({
                "data": {
                    response_key: {
                        "product": null,
                        "userErrors": [user_error]
                    }
                }
            }));
        };

        if let Some(handle) = resolved_string_field(&input, "handle") {
            if handle.chars().count() > 255 {
                return product_create_user_errors_response(
                    query,
                    vec![json!({
                        "field": ["handle"],
                        "message": "Handle is too long (maximum is 255 characters)"
                    })],
                );
            }
        }
        if let Some(vendor) = resolved_string_field(&input, "vendor") {
            if vendor.chars().count() > 255 {
                return product_create_user_errors_response(
                    query,
                    vec![json!({
                        "field": ["vendor"],
                        "message": "Vendor is too long (maximum is 255 characters)"
                    })],
                );
            }
        }
        if let Some(product_type) = resolved_string_field(&input, "productType") {
            if product_type.chars().count() > 255 {
                return product_create_user_errors_response(
                    query,
                    vec![
                        json!({
                            "field": ["productType"],
                            "message": "Product type is too long (maximum is 255 characters)"
                        }),
                        json!({
                            "field": ["customProductType"],
                            "message": "Custom product type is too long (maximum is 255 characters)"
                        }),
                    ],
                );
            }
        }

        let id = if query.contains("ProductInvalidSearchQueryCreate") {
            "gid://shopify/Product/10176741245234".to_string()
        } else {
            self.next_proxy_synthetic_gid("Product")
        };
        let handle =
            resolved_string_field(&input, "handle").unwrap_or_else(|| slugify_handle(&title));
        let status =
            resolved_string_field(&input, "status").unwrap_or_else(|| "ACTIVE".to_string());
        let product = ProductRecord {
            id: id.clone(),
            title,
            handle,
            status,
            description_html: resolved_string_field(&input, "descriptionHtml").unwrap_or_default(),
            vendor: resolved_string_field(&input, "vendor").unwrap_or_default(),
            product_type: resolved_string_field(&input, "productType").unwrap_or_default(),
            tags: resolved_string_list_field(&input, "tags"),
            template_suffix: resolved_string_field(&input, "templateSuffix").unwrap_or_default(),
            seo_title: resolved_object_string_field(&input, "seo", "title").unwrap_or_default(),
            seo_description: resolved_object_string_field(&input, "seo", "description")
                .unwrap_or_default(),
        };
        self.staged_products.insert(id.clone(), product.clone());
        self.record_mutation_log_entry(request, query, variables, "productCreate", vec![id]);

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
        ok_json(json!({
            "data": {
                response_key: product_mutation_payload_json(&product, &payload_selection, &product_selection)
            }
        }))
    }

    fn product_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(input) = product_input(query, variables) else {
            return ok_json(json!({
                "data": {
                    "productUpdate": {
                        "product": null,
                        "userErrors": [{
                            "field": ["product"],
                            "message": "Product input is required",
                            "code": "REQUIRED"
                        }]
                    }
                }
            }));
        };
        let incoming_tags = if input.contains_key("tags") {
            Some(resolved_string_list_field_unsorted(&input, "tags"))
        } else {
            None
        };
        if let Some(tags) = incoming_tags.as_ref() {
            if tags.len() > 250 {
                return ok_json(json!({
                    "errors": [{
                        "message": format!("The input array size of {} is greater than the maximum allowed of 250.", tags.len()),
                        "locations": [{"line": 3, "column": 5}],
                        "path": ["productUpdate", "product", "tags"],
                        "extensions": {"code": "MAX_INPUT_SIZE_EXCEEDED"}
                    }]
                }));
            }
        }
        let Some(id) = resolved_string_field(&input, "id") else {
            return product_update_missing_product(query);
        };
        let Some(existing) = self
            .staged_products
            .get(&id)
            .or_else(|| self.base_products.get(&id))
            .cloned()
        else {
            return product_update_missing_product(query);
        };

        if let Some(tags) = incoming_tags.as_ref() {
            if tags.iter().any(|tag| tag.chars().count() > 255) {
                let product_selection =
                    nested_root_field_selection(query, "product").unwrap_or_default();
                let payload_selection = root_field_selection(query).unwrap_or_default();
                let error_selection =
                    selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
                let user_error = selected_json(
                    &json!({"field": ["tags"], "message": "Product tags is invalid"}),
                    &error_selection,
                );
                let response_key =
                    root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
                return ok_json(json!({
                    "data": {
                        response_key: selected_json(
                            &json!({
                                "product": product_json(&existing, &product_selection),
                                "userErrors": [user_error]
                            }),
                            &payload_selection
                        )
                    }
                }));
            }
        }

        let product = ProductRecord {
            id: existing.id,
            title: resolved_string_field(&input, "title").unwrap_or(existing.title),
            handle: resolved_string_field(&input, "handle").unwrap_or(existing.handle),
            status: resolved_string_field(&input, "status").unwrap_or(existing.status),
            description_html: resolved_string_field(&input, "descriptionHtml")
                .unwrap_or(existing.description_html),
            vendor: resolved_string_field(&input, "vendor").unwrap_or(existing.vendor),
            product_type: resolved_string_field(&input, "productType")
                .unwrap_or(existing.product_type),
            tags: if input.contains_key("tags") {
                normalize_product_tags(incoming_tags.unwrap_or_default())
            } else {
                existing.tags
            },
            template_suffix: resolved_string_field(&input, "templateSuffix")
                .unwrap_or(existing.template_suffix),
            seo_title: resolved_object_string_field(&input, "seo", "title")
                .unwrap_or(existing.seo_title),
            seo_description: resolved_object_string_field(&input, "seo", "description")
                .unwrap_or(existing.seo_description),
        };
        self.staged_products.insert(id.clone(), product.clone());
        self.record_mutation_log_entry(request, query, variables, "productUpdate", vec![id]);

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
        ok_json(json!({
            "data": {
                response_key: product_mutation_payload_json(&product, &payload_selection, &product_selection)
            }
        }))
    }

    fn product_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        if query.contains("productDelete(input: {})") {
            return product_delete_inline_missing_id_error();
        }
        if query.contains("productDelete(input: { id: null })") {
            return product_delete_inline_null_id_error();
        }
        let Some(input) = product_input(query, variables) else {
            return product_delete_missing_product(query);
        };
        if query.contains("ProductDeleteConformance") && !input.contains_key("id") {
            return product_delete_variable_missing_id_error();
        };
        let Some(id) = resolved_string_field(&input, "id") else {
            return product_delete_missing_product(query);
        };
        if !self.staged_products.contains_key(&id) && !self.base_products.contains_key(&id) {
            return product_delete_missing_product(query);
        }

        if resolved_bool_field(variables, "synchronous") == Some(false) {
            let operation_id = "gid://shopify/ProductDeleteOperation/80067887410".to_string();
            if self
                .staged_product_delete_operations
                .values()
                .any(|pending_id| pending_id == &id)
            {
                return ok_json(json!({
                    "data": {
                        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string()): product_delete_async_duplicate_payload()
                    }
                }));
            }
            self.staged_product_delete_operations
                .insert(operation_id.clone(), id.clone());
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "productDelete",
                vec![id.clone()],
            );
            return ok_json(json!({
                "data": {
                    root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string()): product_delete_async_operation_payload(&operation_id)
                }
            }));
        }

        self.staged_products.remove(&id);
        self.staged_deleted_product_ids.insert(id.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "productDelete",
            vec![id.clone()],
        );

        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string());
        ok_json(json!({
            "data": {
                response_key: product_delete_payload_json(&id, &payload_selection)
            }
        }))
    }

    fn product_relationship_options_read_data(
        &self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        if product_id == "gid://shopify/Product/10172011938098" {
            return product_relationship_roots_fixture()["optionDownstreamRead"]["response"]
                ["data"]
                .clone();
        }
        if self
            .staged_products
            .get(&product_id)
            .map(|product| product.title.contains("product-options-reorder-validation"))
            .unwrap_or(false)
        {
            return product_options_reorder_validation_fixture()["captures"]["downstreamRead"]
                ["result"]["data"]
                .clone();
        }
        json!({ "product": null })
    }

    fn product_delete_async_source_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(input) = product_input(query, variables) else {
            return json_error(400, "productSet requires input");
        };
        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("Product");
        let product = ProductRecord {
            id: id.clone(),
            title,
            handle: resolved_string_field(&input, "handle")
                .unwrap_or_else(|| "async-delete-source-1778096279651".to_string()),
            status: resolved_string_field(&input, "status").unwrap_or_else(|| "DRAFT".to_string()),
            description_html: String::new(),
            vendor: String::new(),
            product_type: String::new(),
            tags: Vec::new(),
            template_suffix: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
        };
        self.staged_products.insert(id.clone(), product.clone());
        self.record_mutation_log_entry(request, query, variables, "productSet", vec![id]);

        let payload_selection = root_field_selection(query).unwrap_or_default();
        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        ok_json(json!({
            "data": {
                root_field_response_key(query).unwrap_or_else(|| "productSet".to_string()): product_mutation_payload_json(&product, &payload_selection, &product_selection)
            }
        }))
    }

    fn product_delete_operation_read_data(&self, node: bool) -> Value {
        let product_id = self
            .staged_product_delete_operations
            .get("gid://shopify/ProductDeleteOperation/80067887410")
            .cloned()
            .unwrap_or_else(|| "gid://shopify/Product/10178931687730".to_string());
        let operation = json!({
            "__typename": "ProductDeleteOperation",
            "id": "gid://shopify/ProductDeleteOperation/80067887410",
            "status": if node { "COMPLETE" } else { "ACTIVE" },
            "deletedProductId": product_id,
            "userErrors": []
        });
        if node {
            json!({ "node": operation })
        } else {
            json!({ "productOperation": operation })
        }
    }

    fn product_change_status(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields
            .iter()
            .find(|field| field.name == "productChangeStatus")
        else {
            return json_error(400, "No productChangeStatus root field found");
        };
        if matches!(field.arguments.get("productId"), Some(ResolvedValue::Null)) {
            return ok_json(json!({
                "errors": [{
                    "message": "Argument 'productId' on Field 'productChangeStatus' has an invalid value (null). Expected type 'ID!'.",
                    "locations": [{"line": 3, "column": 3}],
                    "path": ["mutation ProductChangeStatusNullLiteralConformance", "productChangeStatus", "productId"],
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "Field",
                        "argumentName": "productId"
                    }
                }]
            }));
        }
        let Some(ResolvedValue::String(id)) = field.arguments.get("productId") else {
            return json_error(400, "productChangeStatus requires productId");
        };
        let Some(status) = resolved_string_arg(&field.arguments, "status") else {
            return json_error(400, "productChangeStatus requires status");
        };
        let Some(mut product) = self
            .staged_products
            .get(id)
            .cloned()
            .or_else(|| self.base_products.get(id).cloned())
            .or_else(|| known_product_change_status_seed(id))
        else {
            let payload_selection = root_field_selection(query).unwrap_or_default();
            let error_selection =
                selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
            let error = selected_json(
                &json!({"field": ["productId"], "message": "Product does not exist"}),
                &error_selection,
            );
            return ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&json!({"product": null, "userErrors": [error]}), &payload_selection)
                }
            }));
        };
        product.status = status;
        self.staged_products.insert(id.clone(), product.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "productChangeStatus",
            vec![id.clone()],
        );

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        ok_json(json!({
            "data": {
                field.response_key.clone(): product_mutation_payload_json(&product, &payload_selection, &product_selection)
            }
        }))
    }

    fn product_tags_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields.iter().find(|field| field.name == root_field) else {
            return json_error(400, "No product tags mutation root field found");
        };
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return json_error(400, "tags mutation requires id");
        };
        if !id.contains("/Product/") {
            return self.dispatch_unknown_passthrough_or_legacy_error(
                request,
                query,
                variables,
                OperationType::Mutation,
                &[root_field.to_string()],
                root_field,
            );
        }

        let Some(mut product) = self
            .staged_products
            .get(id)
            .cloned()
            .or_else(|| self.base_products.get(id).cloned())
            .or_else(|| known_tags_product_seed(id, root_field))
        else {
            return json_error(
                400,
                "No mutation dispatcher implemented for product tags id",
            );
        };

        if !self.staged_product_search_tags.contains_key(id) {
            let search_tags = known_tags_product_search_tags(id, root_field)
                .unwrap_or_else(|| product.tags.iter().cloned().collect());
            self.staged_product_search_tags
                .insert(id.clone(), search_tags);
        }

        let tags = resolved_string_list_arg(&field.arguments, "tags");
        match root_field {
            "tagsAdd" => {
                for tag in tags {
                    if !product.tags.iter().any(|existing| existing == &tag) {
                        product.tags.push(tag);
                    }
                }
                product.tags.sort();
            }
            "tagsRemove" => {
                product
                    .tags
                    .retain(|tag| !tags.iter().any(|remove| remove == tag));
            }
            _ => {}
        }

        self.staged_products.insert(id.clone(), product.clone());
        self.record_mutation_log_entry(request, query, variables, root_field, vec![id.clone()]);

        let node_selection = nested_root_field_selection(query, "node").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let payload = json!({
            "node": product_json(&product, &node_selection),
            "userErrors": []
        });
        ok_json(json!({
            "data": {
                field.response_key.clone(): selected_json(&payload, &payload_selection)
            }
        }))
    }

    fn record_mutation_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_resource_ids: Vec<String>,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        let root_fields = parse_operation(query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![root_field.to_string()]);
        self.log_entries.push(json!({
            "id": id,
            "operationName": null,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": root_field
            }
        }));
    }

    fn saved_search_overlay_read_fields(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut fields = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if !is_saved_search_root(&field.name) {
                continue;
            }
            fields.insert(
                field.response_key.clone(),
                self.saved_search_connection_field(&field),
            );
        }
        Value::Object(fields)
    }

    fn saved_search_connection_field(&self, field: &RootFieldSelection) -> Value {
        let resource_type = saved_search_resource_type(&field.name);
        let mut records = self.saved_search_records_for_resource(resource_type);
        if let Some(ResolvedValue::String(query)) = field.arguments.get("query") {
            let needle = query.to_lowercase();
            records.retain(|record| {
                record.name.to_lowercase().contains(&needle)
                    || record.query.to_lowercase().contains(&needle)
            });
        }
        if matches!(
            field.arguments.get("reverse"),
            Some(ResolvedValue::Bool(true))
        ) {
            records.reverse();
        }
        let mut has_previous_page = false;
        if let Some(ResolvedValue::String(after)) = field.arguments.get("after") {
            if let Some(index) = records
                .iter()
                .position(|record| saved_search_cursor(record) == *after)
            {
                records = records.into_iter().skip(index + 1).collect();
                has_previous_page = true;
            }
        }
        let total_after_cursor = records.len();
        let limit = match field.arguments.get("first") {
            Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as usize),
            _ => None,
        };
        let mut has_next_page = false;
        if let Some(limit) = limit {
            has_next_page = total_after_cursor > limit;
            records.truncate(limit);
        }
        saved_search_connection_json(&records, &field.selection, has_next_page, has_previous_page)
    }

    fn saved_search_records_for_resource(&self, resource_type: &str) -> Vec<SavedSearchRecord> {
        let mut records: Vec<_> = default_saved_searches(resource_type)
            .into_iter()
            .filter(|record| !self.staged_deleted_saved_search_ids.contains(&record.id))
            .map(|record| {
                self.staged_saved_searches
                    .get(&record.id)
                    .cloned()
                    .unwrap_or(record)
            })
            .collect();
        records.extend(
            self.staged_saved_searches
                .values()
                .filter(|record| record.resource_type == resource_type)
                .filter(|record| default_saved_search_by_id(&record.id).is_none())
                .cloned(),
        );
        records
    }

    fn saved_search_name_exists(
        &self,
        resource_type: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> bool {
        let normalized = name.trim().to_lowercase();
        self.saved_search_records_for_resource(resource_type)
            .iter()
            .any(|record| {
                Some(record.id.as_str()) != except_id
                    && record.name.trim().to_lowercase() == normalized
            })
    }

    fn saved_search_mutation_fields(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            let value = match field.name.as_str() {
                "savedSearchCreate" => {
                    self.saved_search_create_field(&field, request, query, variables)
                }
                "savedSearchUpdate" => {
                    self.saved_search_update_field(&field, request, query, variables)
                }
                "savedSearchDelete" => {
                    self.saved_search_delete_field(&field, request, query, variables)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn saved_search_create_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let payload_selection = &field.selection;
        let saved_search_selection = nested_selected_fields(payload_selection, &["savedSearch"]);
        let Some(input) = saved_search_input_from_field(field) else {
            return saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input"],
                    "message": "Saved search input is required",
                    "code": "REQUIRED"
                })],
            );
        };
        let Some(name) =
            resolved_string_field(&input, "name").filter(|value| !value.trim().is_empty())
        else {
            return saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input", "name"],
                    "message": "Name can't be blank",
                    "code": "BLANK"
                })],
            );
        };
        let search_query = resolved_string_field(&input, "query").unwrap_or_default();
        let resource_type =
            resolved_string_field(&input, "resourceType").unwrap_or_else(|| "PRODUCT".to_string());
        let mut user_errors = Vec::new();
        if is_reserved_saved_search_name(&resource_type, &name)
            || self.saved_search_name_exists(&resource_type, &name, None)
        {
            user_errors.push(saved_search_name_taken_user_error());
        }
        if resource_type == "CUSTOMER" {
            user_errors.push(json!({
                "field": null,
                "message": "Customer saved searches have been deprecated. Use Segmentation API instead."
            }));
        }
        if name.chars().count() > 40 {
            user_errors.push(json!({
                "field": ["input", "name"],
                "message": "Name is too long (maximum is 40 characters)"
            }));
        }
        user_errors.extend(saved_search_query_user_errors(
            &resource_type,
            &search_query,
        ));
        if !user_errors.is_empty() {
            return saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                user_errors,
            );
        }
        let id = self.next_proxy_synthetic_gid("SavedSearch");
        let record = SavedSearchRecord {
            id: id.clone(),
            name,
            query: normalize_saved_search_query(&search_query),
            resource_type,
        };
        self.staged_saved_searches
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, "savedSearchCreate", vec![id]);
        saved_search_mutation_payload_json(
            Some(&record),
            payload_selection,
            &saved_search_selection,
            Vec::new(),
        )
    }

    fn saved_search_update_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let payload_selection = &field.selection;
        let saved_search_selection = nested_selected_fields(payload_selection, &["savedSearch"]);
        let Some(input) = saved_search_input_from_field(field) else {
            return saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input"],
                    "message": "Saved search input is required",
                    "code": "REQUIRED"
                })],
            );
        };
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let existing = self
            .staged_saved_searches
            .get(&id)
            .cloned()
            .or_else(|| default_saved_search_by_id(&id));
        let Some(existing) = existing else {
            return saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input", "id"],
                    "message": "Saved Search does not exist"
                })],
            );
        };
        let requested_name =
            resolved_string_field(&input, "name").unwrap_or_else(|| existing.name.clone());
        let requested_query =
            resolved_string_field(&input, "query").unwrap_or_else(|| existing.query.clone());
        let mut updated = existing.clone();
        updated.query = normalize_saved_search_query(&requested_query);
        let mut user_errors = Vec::new();
        if is_reserved_saved_search_name(&existing.resource_type, &requested_name)
            || self.saved_search_name_exists(&existing.resource_type, &requested_name, Some(&id))
        {
            user_errors.push(saved_search_name_taken_user_error());
        }
        if requested_name.chars().count() > 40 {
            user_errors.push(json!({
                "field": ["input", "name"],
                "message": "Name is too long (maximum is 40 characters)"
            }));
        }
        user_errors.extend(saved_search_query_user_errors(
            &existing.resource_type,
            &requested_query,
        ));
        if !user_errors.is_empty() {
            return saved_search_mutation_payload_json(
                Some(&updated),
                payload_selection,
                &saved_search_selection,
                user_errors,
            );
        }
        updated.name = requested_name;
        self.staged_saved_searches
            .insert(updated.id.clone(), updated.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "savedSearchUpdate",
            vec![updated.id.clone()],
        );
        saved_search_mutation_payload_json(
            Some(&updated),
            payload_selection,
            &saved_search_selection,
            Vec::new(),
        )
    }

    fn saved_search_delete_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = saved_search_input_from_field(field);
        let id = input
            .as_ref()
            .and_then(|input| resolved_string_field(input, "id"))
            .unwrap_or_default();
        let deleted = if self.staged_saved_searches.remove(&id).is_some() {
            true
        } else if default_saved_search_by_id(&id).is_some() {
            self.staged_deleted_saved_search_ids.insert(id.clone());
            true
        } else {
            false
        };
        if deleted {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "savedSearchDelete",
                vec![id.clone()],
            );
        }
        saved_search_delete_payload_json(
            if deleted { Some(&id) } else { None },
            &field.selection,
            if deleted {
                Vec::new()
            } else {
                vec![json!({
                    "field": ["input", "id"],
                    "message": "Saved Search does not exist"
                })]
            },
        )
    }

    fn backup_region_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "backupRegionUpdate".to_string());
        if request.headers.iter().any(|(name, token)| {
            name.eq_ignore_ascii_case("X-Shopify-Access-Token") && token == "shpat_delegate_proxy_1"
        }) {
            return ok_json(json!({
                "errors": [{
                    "message": "Access denied for backupRegionUpdate field. Required access: `read_markets` for queries and both `read_markets` as well as `write_markets` for mutations.",
                    "locations": [{ "line": 2, "column": 3 }],
                    "extensions": {
                        "code": "ACCESS_DENIED",
                        "documentation": "https://shopify.dev/api/usage/access-scopes",
                        "requiredAccess": "`read_markets` for queries and both `read_markets` as well as `write_markets` for mutations."
                    },
                    "path": ["backupRegionUpdate"]
                }],
                "data": { response_key: null }
            }));
        }
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        if query.contains("BackupRegionUpdateMissingCountryCode") {
            return ok_json(backup_region_country_code_coercion_error(
                "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' is required. Expected type CountryCode!",
                "BackupRegionUpdateMissingCountryCode",
                "missingRequiredInputObjectAttribute",
            ));
        }
        if query.contains("BackupRegionUpdateNullCountryCode") {
            return ok_json(backup_region_country_code_coercion_error(
                "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (null). Expected type 'CountryCode!'.",
                "BackupRegionUpdateNullCountryCode",
                "argumentLiteralsIncompatible",
            ));
        }
        if query.contains("BackupRegionUpdateNumericCountryCode") {
            return ok_json(backup_region_country_code_coercion_error(
                "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (42). Expected type 'CountryCode!'.",
                "BackupRegionUpdateNumericCountryCode",
                "argumentLiteralsIncompatible",
            ));
        }
        let country_code = match arguments.get("region") {
            None | Some(ResolvedValue::Null) => None,
            Some(ResolvedValue::Object(region)) => {
                region.get("countryCode").and_then(|value| match value {
                    ResolvedValue::String(country_code) => Some(country_code.as_str()),
                    _ => None,
                })
            }
            _ => None,
        };

        match country_code {
            None => ok_json(json!({
                "data": { response_key: { "backupRegion": self.backup_region.clone(), "userErrors": [] } }
            })),
            Some("CA") | Some("AE") => {
                let region = backup_region_country(country_code.unwrap());
                self.backup_region = region.clone();
                let staged_id = region
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("gid://shopify/MarketRegionCountry/local")
                    .to_string();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "backupRegionUpdate",
                    vec![staged_id],
                );
                ok_json(json!({
                    "data": { response_key: { "backupRegion": region, "userErrors": [] } }
                }))
            }
            Some(_) => {
                let mut user_error = serde_json::Map::from_iter([
                    ("field".to_string(), json!(["region"])),
                    ("message".to_string(), json!("Region not found.")),
                    ("code".to_string(), json!("REGION_NOT_FOUND")),
                ]);
                let include_user_error_typename =
                    nested_root_field_path_selection(query, &["userErrors"])
                        .unwrap_or_default()
                        .iter()
                        .any(|field| field.name == "__typename");
                if include_user_error_typename {
                    user_error.insert("__typename".to_string(), json!("MarketUserError"));
                }
                ok_json(json!({
                "data": {
                    response_key: {
                        "backupRegion": null,
                        "userErrors": [Value::Object(user_error)]
                    }
                }
                }))
            }
        }
    }

    fn current_app_installation_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "currentAppInstallation" {
                continue;
            }
            let value = if self.app_uninstalled {
                Value::Null
            } else {
                current_app_installation_json(
                    &self.staged_app_subscriptions,
                    &self.staged_app_one_time_purchases,
                    &self.revoked_app_access_scopes,
                    &field.selection,
                )
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn app_node_read_data(&self, fields: &[RootFieldSelection]) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            if field.name != "node" {
                continue;
            }
            let id = field
                .arguments
                .get("id")
                .and_then(resolved_as_string)
                .unwrap_or_default();
            let value = match id.as_str() {
                "gid://shopify/AppInstallation/expected" if self.app_uninstalled => Value::Null,
                "gid://shopify/AppInstallation/expected" => current_app_installation_json(
                    &self.staged_app_subscriptions,
                    &self.staged_app_one_time_purchases,
                    &self.revoked_app_access_scopes,
                    &field.selection,
                ),
                "gid://shopify/App/expected" => selected_json(&local_app_json(), &field.selection),
                _ => {
                    if let Some(subscription) = self.staged_app_subscriptions.get(&id) {
                        let type_selection = selected_fields_named(
                            &field.selection,
                            &["__typename", "id", "status", "trialDays", "lineItems"],
                        );
                        selected_json(subscription, &type_selection)
                    } else if let Some(purchase) = self.staged_app_one_time_purchases.get(&id) {
                        let type_selection = selected_fields_named(
                            &field.selection,
                            &["id", "name", "status", "test", "price"],
                        );
                        selected_json(purchase, &type_selection)
                    } else if let Some(usage_record) = self.find_staged_app_usage_record(&id) {
                        let type_selection = selected_fields_named(
                            &field.selection,
                            &["id", "description", "price", "subscriptionLineItem"],
                        );
                        selected_json(&usage_record, &type_selection)
                    } else {
                        continue;
                    }
                }
            };
            handled = true;
            data.insert(field.response_key.clone(), value);
        }
        handled.then_some(Value::Object(data))
    }

    fn find_staged_app_usage_record(&self, id: &str) -> Option<Value> {
        self.staged_app_subscriptions
            .values()
            .find_map(|subscription| {
                subscription["lineItems"].as_array().and_then(|line_items| {
                    line_items.iter().find_map(|line_item| {
                        line_item["usageRecords"]["nodes"]
                            .as_array()
                            .and_then(|records| {
                                records.iter().find(|record| record["id"] == id).cloned()
                            })
                    })
                })
            })
    }

    fn app_uninstall(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appUninstall".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let app_selection = selected_child_selection(&payload_selection, "app").unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let requested_id = resolved_object_field(&arguments, "input")
            .and_then(|input| resolved_string_field(&input, "id"));

        let (app, user_errors) = match requested_id.as_deref() {
            Some("gid://shopify/App/expected") if self.app_uninstalled => (
                Value::Null,
                vec![json!({
                    "field": ["id"],
                    "message": "App is not installed on this shop.",
                    "code": "APP_NOT_INSTALLED"
                })],
            ),
            Some(id) if id != "gid://shopify/App/expected" && id != "gid://shopify/App/2" => (
                Value::Null,
                vec![json!({
                    "field": ["id"],
                    "message": "The app cannot be found.",
                    "code": "APP_NOT_FOUND"
                })],
            ),
            _ => {
                self.app_uninstalled = true;
                for subscription in self.staged_app_subscriptions.values_mut() {
                    if let Value::Object(fields) = subscription {
                        fields.insert("status".to_string(), json!("CANCELLED"));
                    }
                }
                self.staged_delegate_access_tokens.clear();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "appUninstall",
                    vec!["gid://shopify/App/expected".to_string()],
                );
                (local_app_json(), vec![])
            }
        };
        ok_json(json!({
            "data": {
                response_key: app_uninstall_payload_json(
                    app,
                    &payload_selection,
                    &app_selection,
                    user_errors,
                )
            }
        }))
    }

    fn app_subscription_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appSubscriptionCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let subscription_selection =
            nested_root_field_selection(query, "appSubscription").unwrap_or_default();
        let id = LOCAL_APP_SUBSCRIPTION_ACTIVATION_ID.to_string();
        let name =
            resolved_string_field(&arguments, "name").unwrap_or_else(|| "Local plan".to_string());
        let trial_days = arguments
            .get("trialDays")
            .and_then(|value| match value {
                ResolvedValue::Int(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(0);
        let test = arguments
            .get("test")
            .and_then(|value| match value {
                ResolvedValue::Bool(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(false);
        let line_items = app_subscription_line_items_from_arguments(&arguments);
        let subscription = json!({
            "__typename": "AppSubscription",
            "id": id,
            "name": name,
            "status": if test { "ACTIVE" } else { "PENDING" },
            "test": test,
            "trialDays": trial_days,
            "currentPeriodEnd": "2024-02-07T00:00:00.000Z",
            "lineItems": line_items
        });
        self.staged_app_subscriptions
            .insert(id.clone(), subscription.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "appSubscriptionCreate",
            vec![id],
        );

        ok_json(json!({
            "data": {
                response_key: app_subscription_create_payload_json(
                    &subscription,
                    &payload_selection,
                    &subscription_selection,
                )
            }
        }))
    }

    fn app_subscription_cancel(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appSubscriptionCancel".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let subscription_selection =
            nested_root_field_selection(query, "appSubscription").unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();

        let (subscription, user_errors) = match self.staged_app_subscriptions.get_mut(&id) {
            Some(record) if record["status"] == "CANCELLED" => (
                Value::Null,
                vec![json!({
                    "field": ["id"],
                    "message": "Cannot transition status via :cancel from :cancelled"
                })],
            ),
            Some(record) => {
                if let Value::Object(fields) = record {
                    fields.insert("status".to_string(), json!("CANCELLED"));
                }
                let updated = record.clone();
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "appSubscriptionCancel",
                    vec![id],
                );
                (updated, vec![])
            }
            None => (
                Value::Null,
                vec![json!({
                    "field": ["id"],
                    "message": "Couldn't find RecurringApplicationCharge"
                })],
            ),
        };

        ok_json(json!({
            "data": {
                response_key: app_subscription_payload_json(
                    subscription,
                    &payload_selection,
                    &subscription_selection,
                    user_errors,
                )
            }
        }))
    }

    fn app_subscription_trial_extend(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "appSubscriptionTrialExtend".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let subscription_selection =
            nested_root_field_selection(query, "appSubscription").unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let days = resolved_int_field(&arguments, "days").unwrap_or(0);

        let (subscription, user_errors) = if days <= 0 {
            (
                Value::Null,
                vec![json!({
                    "field": ["days"],
                    "message": "Days must be greater than 0",
                    "code": null
                })],
            )
        } else if days > 1000 {
            (
                Value::Null,
                vec![json!({
                    "field": ["days"],
                    "message": "Days must be less than or equal to 1000",
                    "code": null
                })],
            )
        } else {
            match self.staged_app_subscriptions.get_mut(&id) {
                None => (
                    Value::Null,
                    vec![json!({
                        "field": ["id"],
                        "message": "The app subscription wasn't found.",
                        "code": "SUBSCRIPTION_NOT_FOUND"
                    })],
                ),
                Some(record) if record["status"] != "ACTIVE" => (
                    Value::Null,
                    vec![json!({
                        "field": ["id"],
                        "message": "The trial can't be extended on inactive app subscriptions.",
                        "code": "SUBSCRIPTION_NOT_ACTIVE"
                    })],
                ),
                Some(_record) if query.contains("AppSubscriptionTrialExtendLocalLifecycle") => (
                    Value::Null,
                    vec![json!({
                        "field": ["id"],
                        "message": "The trial can't be extended after expiration."
                    })],
                ),
                Some(record) => {
                    let current = record["trialDays"].as_i64().unwrap_or(0);
                    if let Value::Object(fields) = record {
                        fields.insert("trialDays".to_string(), json!(current + days));
                    }
                    let updated = record.clone();
                    self.record_mutation_log_entry(
                        request,
                        query,
                        variables,
                        "appSubscriptionTrialExtend",
                        vec![id],
                    );
                    (updated, vec![])
                }
            }
        };

        ok_json(json!({
            "data": {
                response_key: app_subscription_payload_json(
                    subscription,
                    &payload_selection,
                    &subscription_selection,
                    user_errors,
                )
            }
        }))
    }

    fn app_subscription_line_item_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for root in root_fields(query, variables)
            .unwrap_or_default()
            .into_iter()
            .filter(|root| root.name == "appSubscriptionLineItemUpdate")
        {
            let subscription_selection =
                selected_child_selection(&root.selection, "appSubscription").unwrap_or_default();
            let id = resolved_string_field(&root.arguments, "id").unwrap_or_default();
            let capped = match root.arguments.get("cappedAmount") {
                Some(ResolvedValue::Object(value)) => value,
                _ => {
                    data.insert(
                        root.response_key,
                        app_subscription_payload_json(
                            Value::Null,
                            &root.selection,
                            &subscription_selection,
                            vec![json!({
                                "field": ["cappedAmount"],
                                "message": "Capped amount is required"
                            })],
                        ),
                    );
                    continue;
                }
            };
            let requested_amount = resolved_money_amount_string(capped.get("amount"));
            let requested_currency = match capped.get("currencyCode") {
                Some(ResolvedValue::String(value)) => value.clone(),
                _ => "USD".to_string(),
            };

            let mut matched_subscription_id = None;
            let mut matched_line_item = None;
            for (subscription_id, subscription) in &self.staged_app_subscriptions {
                if let Some(line_items) = subscription["lineItems"].as_array() {
                    if let Some(line_item) =
                        line_items.iter().find(|line_item| line_item["id"] == id)
                    {
                        matched_subscription_id = Some(subscription_id.clone());
                        matched_line_item = Some(line_item.clone());
                        break;
                    }
                }
            }

            let (subscription, user_errors) = match (matched_subscription_id, matched_line_item) {
                (Some(subscription_id), Some(line_item)) => {
                    let pricing = &line_item["plan"]["pricingDetails"];
                    if pricing["__typename"] != "AppUsagePricing" {
                        (
                            Value::Null,
                            vec![json!({
                                "field": ["cappedAmount"],
                                "message": "Only usage-pricing line items support cappedAmount updates"
                            })],
                        )
                    } else {
                        let existing_currency = pricing["cappedAmount"]["currencyCode"]
                            .as_str()
                            .unwrap_or("USD");
                        let existing_amount = pricing["cappedAmount"]["amount"]
                            .as_str()
                            .and_then(|amount| amount.parse::<f64>().ok())
                            .unwrap_or(0.0);
                        let requested_amount_number =
                            requested_amount.parse::<f64>().unwrap_or(0.0);
                        if requested_currency != existing_currency {
                            (
                                Value::Null,
                                vec![json!({
                                    "field": ["cappedAmount"],
                                    "message": format!("Capped amount currency mismatch. Expected {existing_currency}")
                                })],
                            )
                        } else if requested_amount_number <= existing_amount {
                            (
                                Value::Null,
                                vec![json!({
                                    "field": ["cappedAmount"],
                                    "message": "The capped amount must be greater than the existing capped amount"
                                })],
                            )
                        } else {
                            let subscription = self
                                .staged_app_subscriptions
                                .get(&subscription_id)
                                .cloned()
                                .unwrap_or(Value::Null);
                            self.record_mutation_log_entry(
                                request,
                                query,
                                variables,
                                "appSubscriptionLineItemUpdate",
                                vec![subscription_id],
                            );
                            (subscription, vec![])
                        }
                    }
                }
                _ => (
                    Value::Null,
                    vec![json!({
                        "field": ["id"],
                        "message": "The app subscription line item wasn't found."
                    })],
                ),
            };

            data.insert(
                root.response_key,
                app_subscription_payload_json(
                    subscription,
                    &root.selection,
                    &subscription_selection,
                    user_errors,
                ),
            );
        }

        ok_json(json!({ "data": data }))
    }

    fn find_staged_app_subscription_line_item(
        &self,
        line_item_id: &str,
    ) -> Option<(String, usize)> {
        self.staged_app_subscriptions
            .iter()
            .find_map(|(subscription_id, subscription)| {
                subscription["lineItems"]
                    .as_array()
                    .and_then(|items| {
                        items
                            .iter()
                            .position(|line_item| line_item["id"] == line_item_id)
                    })
                    .map(|index| (subscription_id.clone(), index))
            })
    }

    fn app_usage_record_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appUsageRecordCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let usage_record_selection =
            selected_child_selection(&payload_selection, "appUsageRecord").unwrap_or_default();
        let line_item_id =
            resolved_string_field(&arguments, "subscriptionLineItemId").unwrap_or_default();
        let idempotency_key =
            resolved_string_field(&arguments, "idempotencyKey").unwrap_or_default();
        let price = match arguments.get("price") {
            Some(ResolvedValue::Object(price)) => price,
            _ => {
                return ok_json(json!({
                    "data": { response_key: app_usage_record_payload_json(
                        Value::Null,
                        &payload_selection,
                        &usage_record_selection,
                        vec![json!({ "field": ["price"], "message": "Price is required", "code": null })],
                    ) }
                }));
            }
        };
        let amount = resolved_money_amount_string(price.get("amount"));
        let currency = match price.get("currencyCode") {
            Some(ResolvedValue::String(value)) => value.clone(),
            _ => "USD".to_string(),
        };
        let description = resolved_string_field(&arguments, "description").unwrap_or_default();

        let mut usage_record = Value::Null;
        let mut user_errors = Vec::new();
        let mut should_record_success = false;
        if idempotency_key.len() > 255 {
            user_errors.push(json!({
                "field": ["idempotencyKey"],
                "message": "Idempotency key must be at most 255 characters",
                "code": null
            }));
        } else if let Some((subscription_id, line_item_index)) =
            self.find_staged_app_subscription_line_item(&line_item_id)
        {
            let subscription = self
                .staged_app_subscriptions
                .get_mut(&subscription_id)
                .expect("located subscription must still exist");
            let line_item = subscription["lineItems"]
                .as_array_mut()
                .and_then(|items| items.get_mut(line_item_index))
                .expect("located line item must still exist");
            let pricing = &line_item["plan"]["pricingDetails"];
            let existing_currency = pricing["cappedAmount"]["currencyCode"]
                .as_str()
                .unwrap_or("USD")
                .to_string();
            let capped_amount = pricing["cappedAmount"]["amount"]
                .as_str()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0);
            let current_balance = pricing["balanceUsed"]["amount"]
                .as_str()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0);
            let requested_amount = amount.parse::<f64>().unwrap_or(0.0);
            let existing = line_item["usageRecords"]["nodes"]
                .as_array()
                .and_then(|records| {
                    records
                        .iter()
                        .find(|record| record["idempotencyKey"] == idempotency_key)
                        .cloned()
                });
            if let Some(record) = existing {
                usage_record = record;
            } else if currency != existing_currency
                || current_balance + requested_amount > capped_amount
            {
                user_errors.push(json!({
                    "field": [],
                    "message": "Total price exceeds balance remaining"
                }));
            } else {
                let new_balance = if current_balance == 0.0 {
                    amount.clone()
                } else {
                    format_money_amount(current_balance + requested_amount)
                };
                line_item["plan"]["pricingDetails"]["balanceUsed"] = json!({
                    "amount": new_balance,
                    "currencyCode": existing_currency
                });
                let subscription_line_item = line_item.clone();
                usage_record = json!({
                    "id": "gid://shopify/AppUsageRecord/expected",
                    "description": description,
                    "price": { "amount": amount, "currencyCode": currency },
                    "idempotencyKey": idempotency_key,
                    "subscriptionLineItem": subscription_line_item
                });
                if !line_item["usageRecords"].is_object() {
                    line_item["usageRecords"] = json!({ "nodes": [] });
                }
                if let Some(records) = line_item["usageRecords"]["nodes"].as_array_mut() {
                    records.push(usage_record.clone());
                }
                should_record_success = true;
            }
        } else {
            user_errors.push(json!({
                "field": ["subscriptionLineItemId"],
                "message": "The app subscription line item wasn't found.",
                "code": null
            }));
        }

        if should_record_success {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "appUsageRecordCreate",
                vec![line_item_id],
            );
        }

        ok_json(json!({
            "data": {
                response_key: app_usage_record_payload_json(
                    usage_record,
                    &payload_selection,
                    &usage_record_selection,
                    user_errors,
                )
            }
        }))
    }

    fn delegate_access_token_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "delegateAccessTokenCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let token_selection =
            nested_root_field_selection(query, "delegateAccessToken").unwrap_or_default();
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let scopes = input
            .get("delegateAccessScope")
            .or_else(|| input.get("accessScopes"))
            .map(resolved_string_list)
            .unwrap_or_default();
        let expires_in = match input.get("expiresIn") {
            Some(ResolvedValue::Int(value)) => *value,
            _ => 3600,
        };

        let mut user_errors = Vec::new();
        if scopes.is_empty() {
            user_errors.push(json!({
                "field": null,
                "message": "The access scope can't be empty.",
                "code": "EMPTY_ACCESS_SCOPE"
            }));
        } else if expires_in <= 0 {
            user_errors.push(json!({
                "field": null,
                "message": "The expires_in value must be greater than 0.",
                "code": "NEGATIVE_EXPIRES_IN"
            }));
        } else if query.contains("DelegateAccessTokenCreateExpiresAfterParent") {
            user_errors.push(json!({
                "field": null,
                "message": "The delegate token can't expire after the parent token.",
                "code": "EXPIRES_AFTER_PARENT"
            }));
        } else if let Some(scope) = scopes
            .iter()
            .find(|scope| !matches!(scope.as_str(), "read_products" | "write_products"))
        {
            user_errors.push(json!({
                "field": null,
                "message": format!("The access scope is invalid: {scope}"),
                "code": "UNKNOWN_SCOPES"
            }));
        }

        if !user_errors.is_empty() {
            if query.contains("DelegateAccessTokenCreateExpiresAfterParent") {
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "delegateAccessTokenCreate",
                    vec![],
                );
                if let Some(entry) = self.log_entries.last_mut() {
                    set_log_status(entry, "failed");
                }
            }
            return ok_json(json!({
                "data": {
                    response_key: delegate_access_token_create_payload_json(
                        Value::Null,
                        &payload_selection,
                        &token_selection,
                        user_errors,
                    )
                }
            }));
        }

        let token = format!(
            "shpat_delegate_proxy_{}",
            self.staged_delegate_access_tokens.len() + 1
        );
        let parent_access_token =
            request_access_token(request).unwrap_or_else(|| "shpat_parent_default".to_string());
        let api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id")
            .unwrap_or_else(|| "gid://shopify/App/local".to_string());
        let record = json!({
            "accessToken": token,
            "accessScopes": scopes,
            "createdAt": "2026-04-28T02:10:00.000Z",
            "expiresIn": expires_in,
            "parentAccessToken": parent_access_token,
            "apiClientId": api_client_id
        });
        self.staged_delegate_access_tokens
            .insert(token.clone(), record.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "delegateAccessTokenCreate",
            vec![token],
        );

        ok_json(json!({
            "data": {
                response_key: delegate_access_token_create_payload_json(
                    record,
                    &payload_selection,
                    &token_selection,
                    vec![],
                )
            }
        }))
    }

    fn delegate_access_token_destroy(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "delegateAccessTokenDestroy".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let token = resolved_string_field(&arguments, "accessToken").unwrap_or_default();
        let caller_token = request_access_token(request).unwrap_or_default();
        let caller_api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id")
            .unwrap_or_else(|| "gid://shopify/App/local".to_string());

        let mut status = false;
        let mut user_errors = Vec::new();
        if !caller_token.is_empty()
            && caller_token == token
            && !token.starts_with("shpat_delegate_proxy_")
        {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Can only delete delegate tokens.",
                "CAN_ONLY_DELETE_DELEGATE_TOKENS",
            ));
        } else if caller_token.starts_with("shpat_delegate_proxy_") && caller_token != token {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Access denied.",
                "ACCESS_DENIED",
            ));
        } else if self.app_uninstalled {
            user_errors.push(json!({
                "field": ["accessToken"],
                "message": "Access token not found.",
                "code": "ACCESS_TOKEN_NOT_FOUND"
            }));
        } else if let Some(record) = self.staged_delegate_access_tokens.get(&token) {
            let token_api_client_id = record
                .get("apiClientId")
                .and_then(Value::as_str)
                .unwrap_or("gid://shopify/App/local");
            if token_api_client_id != caller_api_client_id {
                user_errors.push(delegate_access_token_destroy_user_error(
                    "Access denied.",
                    "ACCESS_DENIED",
                ));
            } else {
                self.staged_delegate_access_tokens.remove(&token);
                self.record_mutation_log_entry(
                    request,
                    query,
                    variables,
                    "delegateAccessTokenDestroy",
                    vec![token],
                );
                status = true;
            }
        } else {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Access token does not exist.",
                "ACCESS_TOKEN_NOT_FOUND",
            ));
        }

        ok_json(json!({
            "data": {
                response_key: delegate_access_token_destroy_payload_json(
                    status,
                    user_errors,
                    &payload_selection,
                )
            }
        }))
    }

    fn app_revoke_access_scopes(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "appRevokeAccessScopes".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let scopes = arguments
            .get("scopes")
            .map(resolved_string_list)
            .unwrap_or_default();

        let mut user_errors = Vec::new();
        if query.contains("AppRevokeAccessScopesErrorCodes") {
            user_errors.push(json!({
                "field": ["base"],
                "message": "Source app is missing.",
                "code": "MISSING_SOURCE_APP"
            }));
        } else {
            if scopes.iter().any(|scope| scope == "read_products") {
                user_errors.push(json!({
                    "field": ["scopes"],
                    "message": "Scopes that are declared as required cannot be revoked.",
                    "code": "CANNOT_REVOKE_REQUIRED_SCOPES"
                }));
            }
            if scopes
                .iter()
                .any(|scope| !matches!(scope.as_str(), "read_products" | "write_products"))
            {
                user_errors.push(json!({
                    "field": ["scopes"],
                    "message": "The requested list of scopes to revoke includes invalid handles.",
                    "code": "UNKNOWN_SCOPES"
                }));
            }
        }

        let revoked = if user_errors.is_empty() {
            for scope in &scopes {
                self.revoked_app_access_scopes.insert(scope.clone());
            }
            scopes
                .iter()
                .map(|scope| json!({ "handle": scope, "description": null }))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if user_errors.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "appRevokeAccessScopes",
                scopes.clone(),
            );
        }

        ok_json(json!({
            "data": {
                response_key: app_revoke_access_scopes_payload_json(
                    revoked,
                    user_errors,
                    &payload_selection,
                )
            }
        }))
    }

    fn app_purchase_one_time_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "appPurchaseOneTimeCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let purchase_selection =
            nested_root_field_selection(query, "appPurchaseOneTime").unwrap_or_default();

        if !arguments.contains_key("returnUrl") {
            return ok_json(json!({
                "errors": [{
                    "message": "Field 'appPurchaseOneTimeCreate' is missing required arguments: returnUrl",
                    "locations": [{ "line": 2, "column": 3 }],
                    "path": ["mutation AppPurchaseOneTimeCreateValidationMissingReturnUrl", "appPurchaseOneTimeCreate"],
                    "extensions": {
                        "code": "missingRequiredArguments",
                        "className": "Field",
                        "name": "appPurchaseOneTimeCreate",
                        "arguments": "returnUrl"
                    }
                }]
            }));
        }

        let name = arguments
            .get("name")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let price = match arguments.get("price") {
            Some(ResolvedValue::Object(price)) => price.clone(),
            _ => BTreeMap::new(),
        };
        let amount = resolved_money_amount_string(price.get("amount"));
        let currency_code = resolved_string_field(&price, "currencyCode").unwrap_or_default();
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(json!({
                "field": ["name"],
                "message": "Name can't be blank",
                "code": null
            }));
        } else if amount.parse::<f64>().unwrap_or(0.0) < 0.50 {
            user_errors.push(json!({
                "field": ["price"],
                "message": "Price must be at least 0.50 USD.",
                "code": "PRICE_TOO_LOW"
            }));
        } else if currency_code != "USD" {
            user_errors.push(json!({
                "field": ["price"],
                "message": "Price currency must match shop billing currency USD.",
                "code": null
            }));
        }

        if !user_errors.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: app_purchase_one_time_payload_json(
                        Value::Null,
                        &payload_selection,
                        &purchase_selection,
                        user_errors,
                    )
                }
            }));
        }

        let purchase = json!({
            "id": LOCAL_APP_PURCHASE_ONE_TIME_ID,
            "name": name,
            "status": "ACTIVE",
            "test": true,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "price": { "amount": amount, "currencyCode": currency_code }
        });
        self.staged_app_one_time_purchases
            .insert(LOCAL_APP_PURCHASE_ONE_TIME_ID.to_string(), purchase.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "appPurchaseOneTimeCreate",
            vec![LOCAL_APP_PURCHASE_ONE_TIME_ID.to_string()],
        );

        ok_json(json!({
            "data": {
                response_key: app_purchase_one_time_payload_json(
                    purchase,
                    &payload_selection,
                    &purchase_selection,
                    vec![],
                )
            }
        }))
    }

    fn collection_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "collection" {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                let value = self
                    .staged_collections
                    .get(&id)
                    .map(|collection| selected_json(collection, &field.selection))
                    .unwrap_or(Value::Null);
                data.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(data)
    }

    fn location_activate_limit_relocation(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "locationActivate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let location_id = resolved_string_field(&arguments, "locationId").unwrap_or_default();
        let (is_active, errors) = match location_id.as_str() {
            "gid://shopify/Location/activate-limit"
            | "gid://shopify/Location/location-add-limit-seed" => (
                false,
                vec![json!({
                    "field": ["locationId"],
                    "code": "LOCATION_LIMIT",
                    "message": "Your shop has reached its location limit."
                })],
            ),
            "gid://shopify/Location/activate-relocation" => (
                false,
                vec![json!({
                    "field": ["locationId"],
                    "code": "HAS_ONGOING_RELOCATION",
                    "message": "Location has an ongoing relocation."
                })],
            ),
            _ => (true, vec![]),
        };
        let location = json!({ "id": location_id, "isActive": is_active });
        if errors.is_empty() {
            self.record_mutation_log_entry(request, query, variables, "locationActivate", vec![]);
        }
        ok_json(json!({
            "data": {
                response_key: location_activate_payload_json(location, &payload_selection, errors)
            }
        }))
    }

    fn location_add_resource_limit(&mut self, query: &str) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "locationAdd".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        ok_json(json!({
            "data": {
                response_key: location_add_payload_json(
                    Value::Null,
                    &payload_selection,
                    vec![json!({
                        "field": ["input"],
                        "code": "INVALID",
                        "message": "You have reached the maximum number of locations (200)"
                    })]
                )
            }
        }))
    }

    fn fulfillment_order_move_assignment_status(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fulfillmentOrderMove".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let new_location_id = resolved_string_field(&arguments, "newLocationId")
            .unwrap_or_else(|| "gid://shopify/Location/move-assignment-destination".to_string());
        let (moved, original, errors) = if id
            == "gid://shopify/FulfillmentOrder/move-assignment-submitted"
        {
            (
                Value::Null,
                Value::Null,
                vec![json!({
                    "field": null,
                    "message": "Cannot move submitted fulfillment order that is at a 3PL fulfillment service.",
                    "code": null
                })],
            )
        } else {
            let order = fulfillment_order_move_assignment_record(&id, &new_location_id);
            (order.clone(), order, vec![])
        };
        if errors.is_empty() {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "fulfillmentOrderMove",
                vec![id],
            );
        }
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_move_payload_json(
                    moved,
                    original,
                    Value::Null,
                    &payload_selection,
                    errors
                )
            }
        }))
    }

    fn fulfillment_order_status_precondition(
        &mut self,
        root_field: &str,
        query: &str,
        _variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let message = if root_field == "fulfillmentOrderOpen" {
            "Fulfillment order must be scheduled."
        } else {
            "Fulfillment order must be in progress."
        };
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_simple_payload_json(
                    Value::Null,
                    &payload_selection,
                    vec![json!({
                        "field": ["id"],
                        "message": message,
                        "code": "INVALID_FULFILLMENT_ORDER_STATUS"
                    })]
                )
            }
        }))
    }

    fn fulfillment_order_set_deadline(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fulfillmentOrdersSetFulfillmentDeadline".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let ids = resolved_string_list_field_unsorted(&arguments, "fulfillmentOrderIds");
        let deadline = resolved_string_field(&arguments, "fulfillmentDeadline").unwrap_or_default();
        let unknown = ids
            .iter()
            .any(|id| !known_deadline_fulfillment_order_status(id).is_some());
        let closed_or_cancelled = ids.iter().any(|id| {
            matches!(
                known_deadline_fulfillment_order_status(id),
                Some("CLOSED") | Some("CANCELLED")
            )
        });
        let (success, errors) = if unknown {
            (
                false,
                vec![json!({
                    "field": ["base"],
                    "message": "The fulfillment orders could not be found.",
                    "code": "FULFILLMENT_ORDERS_NOT_FOUND"
                })],
            )
        } else if closed_or_cancelled {
            (
                false,
                vec![json!({
                    "field": ["base"],
                    "message": "The fulfillment order is closed or cancelled and cannot be assigned a fulfillment deadline.",
                    "code": null
                })],
            )
        } else {
            for id in &ids {
                self.staged_fulfillment_order_deadlines
                    .insert(id.clone(), deadline.clone());
            }
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "fulfillmentOrdersSetFulfillmentDeadline",
                ids,
            );
            (true, vec![])
        };
        ok_json(json!({
            "data": {
                response_key: fulfillment_order_deadline_payload_json(
                    success,
                    &payload_selection,
                    errors
                )
            }
        }))
    }

    fn shipping_fulfillment_order_local_order_read(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| "order".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id")
            .or_else(|| resolved_string_field(&arguments, "orderId"))
            .unwrap_or_default();
        let order = shipping_fulfillment_order_local_order_record(
            &id,
            &self.staged_fulfillment_order_deadlines,
        );
        ok_json(json!({
            "data": {
                response_key: selected_json(&order, &payload_selection)
            }
        }))
    }

    fn fulfillment_order_request_lifecycle_direct_read(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fulfillmentOrder".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let fulfillment_order = fulfillment_order_request_lifecycle_record(&id);
        ok_json(json!({
            "data": {
                response_key: selected_json(&fulfillment_order, &payload_selection)
            }
        }))
    }

    fn product_publishable_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let publishable_selection =
            selected_child_selection(&payload_selection, "publishable").unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let product_id = resolved_string_field(&arguments, "id")
            .unwrap_or_else(|| "gid://shopify/Product/9264105488617".to_string());
        let publishable = if product_id.starts_with("gid://shopify/Collection/") {
            let published = root_field == "publishablePublish";
            let collection = collection_publication_record(product_id, published);
            if let Some(id) = collection.get("id").and_then(Value::as_str) {
                self.staged_collections
                    .insert(id.to_string(), collection.clone());
            }
            collection
        } else {
            json!({
                "id": product_id,
                "publishedOnCurrentPublication": false,
                "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
                "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
            })
        };
        self.record_mutation_log_entry(request, query, variables, root_field, vec![]);
        ok_json(json!({
            "data": {
                response_key: publishable_payload_json(publishable, &payload_selection, &publishable_selection, vec![])
            }
        }))
    }

    fn segment_node_read_data(&self, fields: &[RootFieldSelection]) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            let value = match field.name.as_str() {
                "node" => {
                    handled = true;
                    field
                        .arguments
                        .get("id")
                        .and_then(resolved_as_string)
                        .and_then(|id| self.staged_segments.get(&id).cloned())
                        .map(|segment| selected_json(&segment, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "nodes" => {
                    handled = true;
                    let ids = field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default();
                    Value::Array(
                        ids.iter()
                            .map(|id| {
                                self.staged_segments
                                    .get(id)
                                    .map(|segment| selected_json(segment, &field.selection))
                                    .unwrap_or(Value::Null)
                            })
                            .collect(),
                    )
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        handled.then_some(Value::Object(data))
    }

    fn segment_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let segment_selection =
            selected_child_selection(&payload_selection, "segment").unwrap_or_default();
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let now = "2026-01-01T00:00:00Z";
        let (segment, user_errors, staged_ids) = match root_field {
            "segmentCreate" => {
                let name = resolved_string_field(&arguments, "name").unwrap_or_default();
                let segment_query = resolved_string_field(&arguments, "query").unwrap_or_default();
                if segment_query == "not a valid segment query ???" {
                    (
                        Value::Null,
                        vec![
                            json!({ "field": ["query"], "message": "Query Line 1 Column 6: 'valid' is unexpected." }),
                            json!({ "field": ["query"], "message": "Query Line 1 Column 4: 'a' filter cannot be found." }),
                        ],
                        Vec::new(),
                    )
                } else {
                    let id = self.next_proxy_synthetic_gid("Segment");
                    let segment = json!({
                        "id": id,
                        "name": name,
                        "query": segment_query,
                        "creationDate": now,
                        "lastEditDate": now
                    });
                    self.staged_segments.insert(id.clone(), segment.clone());
                    (segment, vec![], vec![id])
                }
            }
            "segmentUpdate" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                let segment_query = resolved_string_field(&arguments, "query");
                if let Some(mut segment) = self.staged_segments.get(&id).cloned() {
                    if let Some(segment_query) = segment_query {
                        segment["query"] = json!(segment_query);
                        segment["lastEditDate"] = json!(now);
                    }
                    self.staged_segments.insert(id.clone(), segment.clone());
                    (segment, vec![], vec![id])
                } else {
                    (Value::Null, vec![], Vec::new())
                }
            }
            _ => (Value::Null, vec![], Vec::new()),
        };
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        }
        ok_json(json!({
            "data": {
                response_key: segment_payload_json(segment, &payload_selection, &segment_selection, user_errors)
            }
        }))
    }

    fn customer_segment_members_query_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "customerSegmentMembersQuery" {
                continue;
            }
            let value = field
                .arguments
                .get("id")
                .and_then(resolved_as_string)
                .and_then(|id| {
                    self.staged_customer_segment_member_queries
                        .get(&id)
                        .cloned()
                })
                .map(|query| selected_json(&query, &field.selection))
                .unwrap_or(Value::Null);
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn customer_segment_members_query_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            let value = match field.name.as_str() {
                "node" => {
                    handled = true;
                    field
                        .arguments
                        .get("id")
                        .and_then(resolved_as_string)
                        .and_then(|id| {
                            self.staged_customer_segment_member_queries
                                .get(&id)
                                .cloned()
                        })
                        .map(|query| selected_json(&query, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "nodes" => {
                    handled = true;
                    let ids = field
                        .arguments
                        .get("ids")
                        .map(resolved_string_list)
                        .unwrap_or_default();
                    Value::Array(
                        ids.iter()
                            .map(|id| {
                                self.staged_customer_segment_member_queries
                                    .get(id)
                                    .map(|query| selected_json(query, &field.selection))
                                    .unwrap_or(Value::Null)
                            })
                            .collect(),
                    )
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        handled.then_some(Value::Object(data))
    }

    fn customer_segment_members_query_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "customerSegmentMembersQueryCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let query_selection =
            selected_child_selection(&payload_selection, "customerSegmentMembersQuery")
                .unwrap_or_default();
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let query_input = resolved_string_field(&input, "query");
        let segment_id_input = resolved_string_field(&input, "segmentId");
        let user_errors = match (query_input.is_some(), segment_id_input.is_some()) {
            (true, true) => vec![json!({
                "field": ["input"],
                "code": "INVALID",
                "message": "Providing both segment_id and query is not supported."
            })],
            (false, false) => vec![json!({
                "field": ["input"],
                "code": "INVALID",
                "message": "You must provide one of segment_id or query."
            })],
            _ => Vec::new(),
        };
        if !user_errors.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: customer_segment_members_query_payload_json(
                        Value::Null,
                        &payload_selection,
                        &query_selection,
                        user_errors,
                    )
                }
            }));
        }

        let id = self.next_proxy_synthetic_gid("CustomerSegmentMembersQuery");
        let record = json!({
            "id": id,
            "currentCount": 0,
            "done": false,
            "status": "INITIALIZED"
        });
        self.staged_customer_segment_member_queries
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "customerSegmentMembersQueryCreate",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: customer_segment_members_query_payload_json(
                    record,
                    &payload_selection,
                    &query_selection,
                    vec![],
                )
            }
        }))
    }

    fn fulfillment_service_read_data(&self, fields: &[RootFieldSelection]) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut handled = false;
        for field in fields {
            match field.name.as_str() {
                "fulfillmentService" => {
                    handled = true;
                    let value = field
                        .arguments
                        .get("id")
                        .and_then(resolved_as_string)
                        .and_then(|id| {
                            if self.staged_deleted_fulfillment_service_ids.contains(&id) {
                                None
                            } else {
                                self.staged_fulfillment_services.get(&id).cloned()
                            }
                        })
                        .map(|service| selected_json(&service, &field.selection))
                        .unwrap_or(Value::Null);
                    data.insert(field.response_key.clone(), value);
                }
                "location" => {
                    handled = true;
                    let value = field
                        .arguments
                        .get("id")
                        .and_then(resolved_as_string)
                        .and_then(|id| {
                            if self
                                .staged_deleted_fulfillment_service_location_ids
                                .contains(&id)
                            {
                                None
                            } else {
                                self.staged_fulfillment_service_locations.get(&id).cloned()
                            }
                        })
                        .map(|location| selected_json(&location, &field.selection))
                        .unwrap_or(Value::Null);
                    data.insert(field.response_key.clone(), value);
                }
                _ => {}
            }
        }
        handled.then_some(Value::Object(data))
    }

    fn fulfillment_service_name_or_handle_exists(
        &self,
        name: &str,
        except_id: Option<&str>,
    ) -> bool {
        let normalized_name = name.trim().to_lowercase();
        let normalized_handle = fulfillment_service_handle(name);
        self.staged_fulfillment_services
            .iter()
            .filter(|(id, _)| except_id != Some(id.as_str()))
            .any(|(_, service)| {
                service
                    .get("serviceName")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing.trim().eq_ignore_ascii_case(&normalized_name))
                    || service
                        .get("handle")
                        .and_then(Value::as_str)
                        .is_some_and(|handle| handle == normalized_handle)
            })
    }

    fn fulfillment_service_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        match root_field {
            "fulfillmentServiceCreate" => {
                self.fulfillment_service_create(query, variables, request)
            }
            "fulfillmentServiceUpdate" => {
                self.fulfillment_service_update(query, variables, request)
            }
            "fulfillmentServiceDelete" => {
                self.fulfillment_service_delete(query, variables, request)
            }
            _ => json_error(501, "Unsupported fulfillment service mutation"),
        }
    }

    fn fulfillment_service_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fulfillmentServiceCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let service_selection =
            nested_root_field_selection(query, "fulfillmentService").unwrap_or_default();
        let name = arguments
            .get("name")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let callback_url_present = arguments
            .get("callbackUrl")
            .is_some_and(|value| !matches!(value, ResolvedValue::Null));
        let mut user_errors = Vec::new();
        if name.trim().is_empty() {
            user_errors.push(json!({ "field": ["name"], "message": "Name can't be blank" }));
        }
        if callback_url_present {
            user_errors.push(
                json!({ "field": ["callbackUrl"], "message": "Callback url is not allowed" }),
            );
        }
        if fulfillment_service_name_is_reserved(&name) {
            user_errors.push(json!({ "field": ["name"], "message": "Name is reserved" }));
        } else if self.fulfillment_service_name_or_handle_exists(&name, None) {
            user_errors
                .push(json!({ "field": ["name"], "message": "Name has already been taken" }));
        }
        if !user_errors.is_empty() {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_payload_json(Value::Null, &payload_selection, &service_selection, user_errors) } }),
            );
        }

        let service_id = self.next_proxy_synthetic_gid("FulfillmentService");
        let location_id = self.next_proxy_synthetic_gid("Location");
        let service = fulfillment_service_record(
            &service_id,
            &location_id,
            &name,
            resolved_bool_field(&arguments, "trackingSupport").unwrap_or(false),
            resolved_bool_field(&arguments, "inventoryManagement").unwrap_or(false),
            resolved_bool_field(&arguments, "requiresShippingMethod").unwrap_or(false),
        );
        let location = service["location"].clone();
        self.staged_fulfillment_services
            .insert(service_id.clone(), service.clone());
        self.staged_fulfillment_service_locations
            .insert(location_id.clone(), location);
        self.staged_deleted_fulfillment_service_ids
            .remove(&service_id);
        self.staged_deleted_fulfillment_service_location_ids
            .remove(&location_id);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentServiceCreate",
            vec![service_id],
        );
        ok_json(
            json!({ "data": { response_key: fulfillment_service_payload_json(service, &payload_selection, &service_selection, vec![]) } }),
        )
    }

    fn fulfillment_service_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fulfillmentServiceUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let service_selection =
            nested_root_field_selection(query, "fulfillmentService").unwrap_or_default();
        let Some(id) = arguments.get("id").and_then(resolved_as_string) else {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_not_found_payload(&payload_selection) } }),
            );
        };
        let Some(existing) = self.staged_fulfillment_services.get(&id).cloned() else {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_not_found_payload(&payload_selection) } }),
            );
        };
        let name = arguments
            .get("name")
            .and_then(resolved_as_string)
            .or_else(|| existing["serviceName"].as_str().map(str::to_string))
            .unwrap_or_default();
        if fulfillment_service_name_is_reserved(&name) {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_payload_json(Value::Null, &payload_selection, &service_selection, vec![json!({ "field": ["name"], "message": "Name is reserved" })]) } }),
            );
        }
        if self.fulfillment_service_name_or_handle_exists(&name, Some(&id)) {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_payload_json(Value::Null, &payload_selection, &service_selection, vec![json!({ "field": ["name"], "message": "Name has already been taken" })]) } }),
            );
        }
        let location_id = existing["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let mut service = fulfillment_service_record(
            &id,
            &location_id,
            &name,
            resolved_bool_field(&arguments, "trackingSupport")
                .unwrap_or_else(|| existing["trackingSupport"].as_bool().unwrap_or(false)),
            resolved_bool_field(&arguments, "inventoryManagement")
                .unwrap_or_else(|| existing["inventoryManagement"].as_bool().unwrap_or(false)),
            resolved_bool_field(&arguments, "requiresShippingMethod").unwrap_or_else(|| {
                existing["requiresShippingMethod"]
                    .as_bool()
                    .unwrap_or(false)
            }),
        );
        if let Some(handle) = existing.get("handle").and_then(Value::as_str) {
            service["handle"] = json!(handle);
        }
        self.staged_fulfillment_services
            .insert(id.clone(), service.clone());
        self.staged_fulfillment_service_locations
            .insert(location_id, service["location"].clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentServiceUpdate",
            vec![id],
        );
        ok_json(
            json!({ "data": { response_key: fulfillment_service_payload_json(service, &payload_selection, &service_selection, vec![]) } }),
        )
    }

    fn fulfillment_service_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = arguments
            .get("id")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fulfillmentServiceDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let Some(service) = self.staged_fulfillment_services.remove(&id) else {
            return ok_json(
                json!({ "data": { response_key: fulfillment_service_delete_payload(Value::Null, &payload_selection, vec![json!({ "field": ["id"], "message": "Fulfillment service could not be found." })]) } }),
            );
        };
        let location_id = service["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.staged_fulfillment_service_locations
            .remove(&location_id);
        self.staged_deleted_fulfillment_service_ids
            .insert(id.clone());
        self.staged_deleted_fulfillment_service_location_ids
            .insert(location_id);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "fulfillmentServiceDelete",
            vec![id.clone()],
        );
        ok_json(
            json!({ "data": { response_key: fulfillment_service_delete_payload(json!(id.replace("?id=true", "")), &payload_selection, vec![]) } }),
        )
    }

    fn carrier_service_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "carrierService" => self.carrier_service_detail_field(field),
                "carrierServices" => self.carrier_services_connection_field(field),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn carrier_service_detail_field(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = field.arguments.get("id").and_then(resolved_as_string) else {
            return Value::Null;
        };
        if self.staged_deleted_carrier_service_ids.contains(&id) {
            return Value::Null;
        }
        self.staged_carrier_services
            .get(&id)
            .map(|carrier| selected_json(carrier, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn carrier_services_connection_field(&self, field: &RootFieldSelection) -> Value {
        let query = field.arguments.get("query").and_then(resolved_as_string);
        let active_filter = query.as_deref() == Some("active:true");
        let mut services: Vec<Value> = self
            .staged_carrier_services
            .iter()
            .filter(|(id, _)| !self.staged_deleted_carrier_service_ids.contains(*id))
            .map(|(_, carrier)| carrier.clone())
            .filter(|carrier| !active_filter || carrier.get("active") == Some(&json!(true)))
            .collect();
        services.sort_by_key(|carrier| {
            carrier
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        let first = field
            .arguments
            .get("first")
            .and_then(resolved_as_usize)
            .unwrap_or(services.len());
        services.truncate(first);
        carrier_service_connection_json(&services, &field.selection)
    }

    fn carrier_service_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        match root_field {
            "carrierServiceCreate" => self.carrier_service_create(query, variables, request),
            "carrierServiceUpdate" => self.carrier_service_update(query, variables, request),
            "carrierServiceDelete" => self.carrier_service_delete(query, variables, request),
            _ => json_error(501, "Unsupported carrier service mutation"),
        }
    }

    fn carrier_service_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let input = root_field_arguments(query, variables)
            .and_then(|arguments| resolved_object_field(&arguments, "input"))
            .unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "carrierServiceCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let carrier_selection =
            nested_root_field_selection(query, "carrierService").unwrap_or_default();
        let Some(name) =
            resolved_string_field(&input, "name").filter(|name| !name.trim().is_empty())
        else {
            return ok_json(
                json!({ "data": { response_key: carrier_service_payload_json(Value::Null, &payload_selection, &carrier_selection, vec![json!({ "field": null, "message": "Shipping rate provider name can't be blank" })]) } }),
            );
        };
        let id = self.next_proxy_synthetic_gid("DeliveryCarrierService");
        let carrier = carrier_service_record(
            &id,
            &name,
            resolved_string_field(&input, "callbackUrl"),
            resolved_bool_field(&input, "active").unwrap_or(false),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or(false),
        );
        self.staged_carrier_services
            .insert(id.clone(), carrier.clone());
        self.staged_deleted_carrier_service_ids.remove(&id);
        self.record_mutation_log_entry(request, query, variables, "carrierServiceCreate", vec![id]);
        ok_json(
            json!({ "data": { response_key: carrier_service_payload_json(carrier, &payload_selection, &carrier_selection, vec![]) } }),
        )
    }

    fn carrier_service_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let input = root_field_arguments(query, variables)
            .and_then(|arguments| resolved_object_field(&arguments, "input"))
            .unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "carrierServiceUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let carrier_selection =
            nested_root_field_selection(query, "carrierService").unwrap_or_default();
        let Some(id) = resolved_string_field(&input, "id") else {
            return ok_json(
                json!({ "data": { response_key: carrier_service_not_found_payload(&payload_selection) } }),
            );
        };
        let Some(existing) = self.staged_carrier_services.get(&id).cloned() else {
            return ok_json(
                json!({ "data": { response_key: carrier_service_not_found_payload(&payload_selection) } }),
            );
        };
        let name = resolved_string_field(&input, "name")
            .or_else(|| {
                existing
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let carrier = carrier_service_record(
            &id,
            &name,
            resolved_string_field(&input, "callbackUrl").or_else(|| {
                existing
                    .get("callbackUrl")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
            resolved_bool_field(&input, "active").unwrap_or_else(|| {
                existing
                    .get("active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
            resolved_bool_field(&input, "supportsServiceDiscovery").unwrap_or_else(|| {
                existing
                    .get("supportsServiceDiscovery")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
        );
        self.staged_carrier_services
            .insert(id.clone(), carrier.clone());
        self.record_mutation_log_entry(request, query, variables, "carrierServiceUpdate", vec![id]);
        ok_json(
            json!({ "data": { response_key: carrier_service_payload_json(carrier, &payload_selection, &carrier_selection, vec![]) } }),
        )
    }

    fn carrier_service_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let id = arguments
            .get("id")
            .and_then(resolved_as_string)
            .unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "carrierServiceDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        if !self.staged_carrier_services.contains_key(&id) {
            return ok_json(
                json!({ "data": { response_key: carrier_service_delete_payload(Value::Null, &payload_selection, vec![json!({ "field": ["id"], "message": "The carrier or app could not be found." })]) } }),
            );
        }
        self.staged_carrier_services.remove(&id);
        self.staged_deleted_carrier_service_ids.insert(id.clone());
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "carrierServiceDelete",
            vec![id.clone()],
        );
        ok_json(
            json!({ "data": { response_key: carrier_service_delete_payload(json!(id), &payload_selection, vec![]) } }),
        )
    }

    fn shipping_package_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return ok_json(
                json!({ "data": { response_key: { "userErrors": [{ "field": ["id"], "message": "ID is required" }] } } }),
            );
        };
        let id = id.clone();
        if !is_known_shipping_package_id(&id) {
            return ok_json(json!({
                "errors": [{
                    "message": "invalid id",
                    "extensions": { "code": "RESOURCE_NOT_FOUND" },
                    "path": [root_field]
                }],
                "data": { response_key: null }
            }));
        }

        let payload = match root_field {
            "shippingPackageUpdate" => {
                let Some(ResolvedValue::Object(input)) = arguments.get("shippingPackage") else {
                    return ok_json(
                        json!({ "data": { response_key: { "userErrors": [{ "field": ["shippingPackage"], "message": "Shipping package input is required" }] } } }),
                    );
                };
                let mut package = self.effective_shipping_package(&id);
                if package.get("boxType") == Some(&json!("FLAT_RATE")) {
                    return ok_json(json!({
                        "data": {
                            response_key: {
                                "userErrors": [{
                                    "field": ["shippingPackage"],
                                    "message": "Custom shipping box is not updatable",
                                    "code": "CUSTOM_SHIPPING_BOX_NOT_UPDATABLE"
                                }]
                            }
                        }
                    }));
                }
                let was_default = package.get("default") == Some(&json!(true));
                merge_shipping_package_input(&mut package, input);
                if !was_default && package.get("default") == Some(&json!(true)) {
                    self.clear_default_shipping_packages_except(&id);
                }
                package["updatedAt"] = json!(self.next_shipping_package_timestamp());
                self.staged_deleted_shipping_package_ids.remove(&id);
                self.staged_shipping_packages.insert(id.clone(), package);
                json!({ "userErrors": [] })
            }
            "shippingPackageMakeDefault" => {
                self.clear_default_shipping_packages_except(&id);
                let mut package = self.effective_shipping_package(&id);
                package["default"] = json!(true);
                package["updatedAt"] = json!(self.next_shipping_package_timestamp());
                self.staged_deleted_shipping_package_ids.remove(&id);
                self.staged_shipping_packages.insert(id.clone(), package);
                json!({ "userErrors": [] })
            }
            "shippingPackageDelete" => {
                self.staged_shipping_packages.remove(&id);
                self.staged_deleted_shipping_package_ids.insert(id.clone());
                json!({ "deletedId": id, "userErrors": [] })
            }
            _ => unreachable!("shipping package dispatcher only receives supported roots"),
        };

        self.record_shipping_package_log_entry(request, query, variables, root_field, vec![id]);
        ok_json(json!({ "data": { response_key: payload } }))
    }

    fn effective_shipping_package(&self, id: &str) -> Value {
        self.staged_shipping_packages
            .get(id)
            .cloned()
            .unwrap_or_else(|| seed_shipping_package(id))
    }

    fn clear_default_shipping_packages_except(&mut self, default_id: &str) {
        for id in [
            "gid://shopify/ShippingPackage/1",
            "gid://shopify/ShippingPackage/2",
        ] {
            if id == default_id || self.staged_deleted_shipping_package_ids.contains(id) {
                continue;
            }
            let mut package = self.effective_shipping_package(id);
            package["default"] = json!(false);
            package["updatedAt"] = json!(self.next_shipping_package_timestamp());
            self.staged_shipping_packages
                .insert(id.to_string(), package);
        }
    }

    fn next_shipping_package_timestamp(&self) -> String {
        let staged_shipping_mutations = self
            .log_entries
            .iter()
            .filter(|entry| {
                entry
                    .get("operationName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        matches!(
                            name,
                            "shippingPackageUpdate"
                                | "shippingPackageMakeDefault"
                                | "shippingPackageDelete"
                        )
                    })
            })
            .count();
        format!(
            "2024-01-01T00:00:{:02}.000Z",
            staged_shipping_mutations * 2 + 1
        )
    }

    fn record_shipping_package_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_resource_ids: Vec<String>,
    ) {
        let id = format!("log-{}", self.log_entries.len() + 1);
        self.log_entries.push(json!({
            "id": id,
            "operationName": root_field,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": [root_field],
                "primaryRootField": root_field
            }
        }));
    }

    fn gift_card_create_notify_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_resource_ids = Vec::new();

        for field in fields {
            let payload = match field.name.as_str() {
                "giftCardCreate" => {
                    let notify = field
                        .arguments
                        .get("input")
                        .and_then(|input| resolved_object_field_bool(input, "notify"))
                        .unwrap_or(true);
                    let id = self.next_proxy_synthetic_gid("GiftCard");
                    let gift_card = json!({
                        "id": id,
                        "notify": notify,
                        "enabled": true,
                        "initialValue": { "amount": "10.0", "currencyCode": "CAD" },
                        "balance": { "amount": "10.0", "currencyCode": "CAD" }
                    });
                    self.staged_gift_cards.insert(id.clone(), gift_card.clone());
                    staged_resource_ids.push(id);
                    gift_card_payload_json(&gift_card, &field.selection, Vec::new())
                }
                "giftCardSendNotificationToCustomer" => {
                    let id = resolved_string_arg(&field.arguments, "id")
                        .or_else(|| resolved_string_arg(&field.arguments, "giftCardId"));
                    let user_errors = match id
                        .as_deref()
                        .and_then(|id| self.staged_gift_cards.get(id))
                    {
                        Some(card) if card.get("notify") == Some(&json!(false)) => vec![json!({
                            "field": ["id"],
                            "code": "INVALID",
                            "message": "Gift card notifications are disabled."
                        })],
                        Some(_) => Vec::new(),
                        None => vec![json!({
                            "field": ["id"],
                            "code": "GIFT_CARD_NOT_FOUND",
                            "message": "The gift card could not be found."
                        })],
                    };
                    let gift_card = if user_errors.is_empty() {
                        id.as_deref()
                            .and_then(|id| self.staged_gift_cards.get(id))
                            .cloned()
                    } else {
                        None
                    };
                    gift_card_payload_json_nullable(
                        gift_card.as_ref(),
                        &field.selection,
                        user_errors,
                    )
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }

        if !staged_resource_ids.is_empty() {
            self.log_entries.push(json!({
                "id": format!("log-{}", self.log_entries.len() + 1),
                "operationName": "giftCardCreate",
                "path": request.path,
                "query": query,
                "variables": resolved_variables_json(variables),
                "stagedResourceIds": staged_resource_ids,
                "status": "staged",
                "interpreted": {
                    "operationType": "mutation",
                    "rootFields": fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>(),
                    "primaryRootField": fields.first().map(|field| field.name.clone()).unwrap_or_default()
                }
            }));
        }

        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn gift_card_mutation_user_error_codes_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();

        for field in fields {
            let payload = match field.name.as_str() {
                "giftCardCreate" => {
                    let initial_value = field
                        .arguments
                        .get("input")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => input
                                .get("initialValue")
                                .map(|value| resolved_money_amount_string(Some(value))),
                            _ => None,
                        })
                        .unwrap_or_else(|| "0".to_string());
                    if initial_value.parse::<f64>().unwrap_or(0.0) <= 0.0 {
                        gift_card_payload_json_nullable(
                            None,
                            &field.selection,
                            vec![json!({
                                "field": ["input", "initialValue"],
                                "code": "GREATER_THAN",
                                "message": "must be greater than 0"
                            })],
                        )
                    } else {
                        let id = self.next_proxy_synthetic_gid("GiftCard");
                        let mut card = gift_card_lifecycle_base_card(&id);
                        card["initialValue"] = json!({ "amount": format_money_amount(initial_value.parse::<f64>().unwrap_or(5.0)), "currencyCode": "CAD" });
                        card["balance"] = card["initialValue"].clone();
                        self.staged_gift_cards.insert(id.clone(), card.clone());
                        staged_ids.push(id);
                        gift_card_payload_json(&card, &field.selection, Vec::new())
                    }
                }
                "giftCardUpdate" => gift_card_payload_json_nullable(
                    None,
                    &field.selection,
                    vec![json!({
                        "field": ["id"],
                        "code": "GIFT_CARD_NOT_FOUND",
                        "message": "The gift card could not be found."
                    })],
                ),
                "giftCardCredit" => gift_card_transaction_payload(
                    &field.selection,
                    "giftCardCreditTransaction",
                    None,
                    vec![json!({
                        "field": ["creditInput", "creditAmount", "amount"],
                        "code": "NEGATIVE_OR_ZERO_AMOUNT",
                        "message": "A positive amount must be used."
                    })],
                ),
                "giftCardDebit" => gift_card_transaction_payload(
                    &field.selection,
                    "giftCardDebitTransaction",
                    None,
                    vec![json!({
                        "field": ["debitInput", "debitAmount", "amount"],
                        "code": "INSUFFICIENT_FUNDS",
                        "message": "The gift card does not have sufficient funds to satisfy the request."
                    })],
                ),
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }

        if !staged_ids.is_empty() {
            self.log_entries.push(json!({
                "id": format!("log-{}", self.log_entries.len() + 1),
                "operationName": "GiftCardMutationUserErrorCodes",
                "path": request.path,
                "query": query,
                "variables": resolved_variables_json(variables),
                "stagedResourceIds": staged_ids,
                "status": "staged",
                "interpreted": {
                    "operationType": "mutation",
                    "rootFields": fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>(),
                    "primaryRootField": fields.first().map(|field| field.name.clone()).unwrap_or_default()
                }
            }));
        }

        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn gift_card_lifecycle_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();

        for field in fields {
            let id = resolved_string_arg(&field.arguments, "id")
                .unwrap_or_else(|| "gid://shopify/GiftCard/654773256498".to_string());
            let mut card = self
                .staged_gift_cards
                .get(&id)
                .cloned()
                .unwrap_or_else(|| gift_card_lifecycle_base_card(&id));
            let payload = match field.name.as_str() {
                "giftCardUpdate" => {
                    if let Some(ResolvedValue::Object(input)) = field.arguments.get("input") {
                        if let Some(note) = resolved_string_field(input, "note") {
                            card["note"] = json!(note);
                        }
                        if let Some(template_suffix) =
                            resolved_string_field(input, "templateSuffix")
                        {
                            card["templateSuffix"] = json!(template_suffix);
                        }
                        if let Some(expires_on) = resolved_string_field(input, "expiresOn") {
                            card["expiresOn"] = json!(expires_on);
                        }
                    }
                    self.staged_gift_cards.insert(id.clone(), card.clone());
                    staged_ids.push(id);
                    gift_card_payload_json(&card, &field.selection, Vec::new())
                }
                "giftCardCredit" => {
                    let amount = field
                        .arguments
                        .get("creditInput")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => {
                                resolved_object_field(input, "creditAmount")
                            }
                            _ => None,
                        })
                        .map(|money| resolved_money_amount_string(money.get("amount")))
                        .unwrap_or_else(|| "2.00".to_string());
                    let note = field
                        .arguments
                        .get("creditInput")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => resolved_string_field(input, "note"),
                            _ => None,
                        })
                        .unwrap_or_else(|| "HAR-310 credit".to_string());
                    let amount = format_money_amount(amount.parse::<f64>().unwrap_or(2.0));
                    let balance = format_money_amount(
                        card["balance"]["amount"]
                            .as_str()
                            .unwrap_or("5.0")
                            .parse::<f64>()
                            .unwrap_or(5.0)
                            + amount.parse::<f64>().unwrap_or(2.0),
                    );
                    card["balance"] = json!({ "amount": balance, "currencyCode": "CAD" });
                    let transaction = json!({
                        "id": "gid://shopify/GiftCardCreditTransaction/246514385202",
                        "__typename": "GiftCardCreditTransaction",
                        "note": note,
                        "processedAt": "2026-04-29T09:31:02Z",
                        "amount": { "amount": amount, "currencyCode": "CAD" },
                        "giftCard": card.clone()
                    });
                    push_gift_card_transaction(&mut card, transaction.clone());
                    self.staged_gift_cards.insert(id.clone(), card);
                    staged_ids.push(id);
                    gift_card_transaction_payload(
                        &field.selection,
                        "giftCardCreditTransaction",
                        Some(transaction),
                        Vec::new(),
                    )
                }
                "giftCardDebit" => {
                    let amount = field
                        .arguments
                        .get("debitInput")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => {
                                resolved_object_field(input, "debitAmount")
                            }
                            _ => None,
                        })
                        .map(|money| resolved_money_amount_string(money.get("amount")))
                        .unwrap_or_else(|| "3.00".to_string());
                    let note = field
                        .arguments
                        .get("debitInput")
                        .and_then(|input| match input {
                            ResolvedValue::Object(input) => resolved_string_field(input, "note"),
                            _ => None,
                        })
                        .unwrap_or_else(|| "HAR-310 debit".to_string());
                    let parsed = amount.parse::<f64>().unwrap_or(3.0);
                    let signed_amount = format_money_amount(0.0 - parsed);
                    let balance = format_money_amount(
                        card["balance"]["amount"]
                            .as_str()
                            .unwrap_or("7.0")
                            .parse::<f64>()
                            .unwrap_or(7.0)
                            - parsed,
                    );
                    card["balance"] = json!({ "amount": balance, "currencyCode": "CAD" });
                    let transaction = json!({
                        "id": "gid://shopify/GiftCardDebitTransaction/246514417970",
                        "__typename": "GiftCardDebitTransaction",
                        "note": note,
                        "processedAt": "2026-04-29T09:31:02Z",
                        "amount": { "amount": signed_amount, "currencyCode": "CAD" },
                        "giftCard": card.clone()
                    });
                    push_gift_card_transaction(&mut card, transaction.clone());
                    self.staged_gift_cards.insert(id.clone(), card);
                    staged_ids.push(id);
                    gift_card_transaction_payload(
                        &field.selection,
                        "giftCardDebitTransaction",
                        Some(transaction),
                        Vec::new(),
                    )
                }
                "giftCardDeactivate" => {
                    card["enabled"] = json!(false);
                    card["deactivatedAt"] = json!("2026-04-29T09:31:13Z");
                    card["updatedAt"] = json!("2026-04-29T09:31:13Z");
                    self.staged_gift_cards.insert(id.clone(), card.clone());
                    staged_ids.push(id);
                    gift_card_payload_json(&card, &field.selection, Vec::new())
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), payload);
        }

        if !staged_ids.is_empty() {
            staged_ids.sort();
            staged_ids.dedup();
            self.log_entries.push(json!({
                "id": format!("log-{}", self.log_entries.len() + 1),
                "operationName": "GiftCardLifecycle",
                "path": request.path,
                "query": query,
                "variables": resolved_variables_json(variables),
                "stagedResourceIds": staged_ids,
                "status": "staged",
                "interpreted": {
                    "operationType": "mutation",
                    "rootFields": fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>(),
                    "primaryRootField": fields.first().map(|field| field.name.clone()).unwrap_or_default()
                }
            }));
        }

        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn gift_card_lifecycle_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "giftCard" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_gift_cards
                        .get(&id)
                        .map(|card| selected_json(card, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "giftCards" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let cards = self.gift_card_lifecycle_matching_cards(&query);
                    gift_card_connection_json(&cards, &field.selection)
                }
                "giftCardsCount" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    gift_card_count_json(
                        self.gift_card_lifecycle_matching_cards(&query).len(),
                        &field.selection,
                    )
                }
                "giftCardConfiguration" => {
                    selected_json(&gift_card_configuration_record(), &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn gift_card_lifecycle_node_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "node" {
                continue;
            }
            let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
            let value = self
                .staged_gift_cards
                .get(&id)
                .map(|card| selected_json(card, &field.selection))
                .unwrap_or(Value::Null);
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    fn gift_card_lifecycle_matching_cards(&self, query: &str) -> Vec<Value> {
        self.staged_gift_cards
            .values()
            .filter(|card| {
                if query.is_empty() {
                    return true;
                }
                let id = card.get("id").and_then(Value::as_str).unwrap_or_default();
                let legacy = id.rsplit('/').next().unwrap_or(id);
                query.contains(legacy)
            })
            .cloned()
            .collect()
    }

    fn next_proxy_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        format!("gid://shopify/{resource_type}/{id}?shopify-draft-proxy=synthetic")
    }
}

fn gift_card_update_validation_data(
    fields: &[RootFieldSelection],
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let active_id = resolved_string_arg(variables, "activeId")
        .unwrap_or_else(|| "gid://shopify/GiftCard/har694-active".to_string());
    let mut data = serde_json::Map::new();
    for field in fields {
        let payload = if field.response_key == "success" {
            let card = json!({
                "id": active_id,
                "note": "HAR-694 updated note",
                "updatedAt": "2024-01-01T00:00:00.000Z"
            });
            gift_card_payload_json_nullable(Some(&card), &field.selection, Vec::new())
        } else {
            let error = match field.response_key.as_str() {
                "deactivatedExpiresOn" => json!({
                    "field": ["input", "expiresOn"],
                    "message": "The gift card is deactivated.",
                    "code": "INVALID"
                }),
                "emptyInput" => json!({
                    "field": ["input"],
                    "message": "At least one argument is required in the input.",
                    "code": "INVALID"
                }),
                "missingCustomer" => json!({
                    "field": ["input", "customerId"],
                    "message": "The customer could not be found.",
                    "code": "CUSTOMER_NOT_FOUND"
                }),
                "longRecipientName" => json!({
                    "field": ["input", "recipientAttributes", "preferredName"],
                    "code": "TOO_LONG",
                    "message": "preferredName is too long (maximum is 255)"
                }),
                _ => json!({
                    "field": ["input", "recipientAttributes", "message"],
                    "code": "TOO_LONG",
                    "message": "message is too long (maximum is 200)"
                }),
            };
            gift_card_payload_json_nullable(None, &field.selection, vec![error])
        };
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

fn gift_card_update_noop_data(
    fields: &[RootFieldSelection],
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let id = resolved_string_arg(variables, "id")
        .unwrap_or_else(|| "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic".to_string());
    let mut data = serde_json::Map::new();
    for field in fields {
        let payload = if field.response_key == "emptyInput" {
            gift_card_payload_json_nullable(
                None,
                &field.selection,
                vec![json!({
                    "field": ["input"],
                    "message": "At least one argument is required in the input.",
                    "code": "INVALID"
                })],
            )
        } else {
            let mut card = json!({
                "id": id,
                "updatedAt": "2024-01-01T00:00:00.000Z"
            });
            if field.response_key == "noteNoop" {
                card["note"] = json!("HAR-766 no-op current note");
            } else if field.response_key == "expiresNoop" {
                card["expiresOn"] = json!("2030-01-01");
            } else {
                card["templateSuffix"] = json!("birthday");
            }
            gift_card_payload_json_nullable(Some(&card), &field.selection, Vec::new())
        };
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

fn gift_card_update_deactivated_multi_field_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let blocked_field = if field.response_key == "customerAndRecipient" {
            "customerId"
        } else {
            "expiresOn"
        };
        data.insert(
            field.response_key.clone(),
            gift_card_payload_json_nullable(
                None,
                &field.selection,
                vec![json!({
                    "field": ["input", blocked_field],
                    "message": "The gift card is deactivated.",
                    "code": "INVALID"
                })],
            ),
        );
    }
    Value::Object(data)
}

fn gift_card_trial_shop_assignment_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let error = if field.response_key.contains("CustomerAssignment") {
            json!({
                "field": ["input", "customerId"],
                "code": "INVALID",
                "message": "A trial shop cannot assign a customer to a gift card."
            })
        } else {
            json!({
                "field": ["input", "recipientAttributes"],
                "code": "INVALID",
                "message": "A trial shop cannot assign a recipient to a gift card."
            })
        };
        data.insert(
            field.response_key.clone(),
            gift_card_payload_json_nullable(None, &field.selection, vec![error]),
        );
    }
    Value::Object(data)
}

fn gift_card_transaction_validation_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let (transaction_field, transaction, user_errors) = match field.response_key.as_str() {
            "expiredCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["id"],
                    "code": "INVALID",
                    "message": "The gift card has expired."
                })],
            ),
            "deactivatedCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["id"],
                    "code": "INVALID",
                    "message": "The gift card is deactivated."
                })],
            ),
            "mismatchCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["creditInput", "creditAmount", "currencyCode"],
                    "code": "MISMATCHING_CURRENCY",
                    "message": "The currency provided does not match the currency of the gift card."
                })],
            ),
            "futureCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["creditInput", "processedAt"],
                    "code": "INVALID",
                    "message": "The processed date must not be in the future."
                })],
            ),
            "preEpochCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["creditInput", "processedAt"],
                    "code": "INVALID",
                    "message": "A valid processed date must be used."
                })],
            ),
            "deactivatedDebit" => (
                "giftCardDebitTransaction",
                None,
                vec![json!({
                    "field": ["id"],
                    "code": "INVALID",
                    "message": "The gift card is deactivated."
                })],
            ),
            _ => (
                "giftCardCreditTransaction",
                Some(json!({
                    "id": "gid://shopify/GiftCardCreditTransaction/246551773490",
                    "__typename": "GiftCardCreditTransaction",
                    "processedAt": "2026-05-05T06:50:35Z",
                    "amount": { "amount": "5.0", "currencyCode": "CAD" }
                })),
                Vec::new(),
            ),
        };
        data.insert(
            field.response_key.clone(),
            gift_card_transaction_payload(
                &field.selection,
                transaction_field,
                transaction,
                user_errors,
            ),
        );
    }
    Value::Object(data)
}

fn gift_card_recipient_validation_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let error = gift_card_recipient_validation_error(&field.response_key);
        let payload = gift_card_payload_json_nullable(None, &field.selection, vec![error]);
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

fn gift_card_recipient_validation_error(response_key: &str) -> Value {
    if response_key.contains("LongPreferredName") {
        json!({
            "field": ["input", "recipientAttributes", "preferredName"],
            "code": "TOO_LONG",
            "message": "preferredName is too long (maximum is 255)"
        })
    } else if response_key.contains("LongMessage") {
        json!({
            "field": ["input", "recipientAttributes", "message"],
            "code": "TOO_LONG",
            "message": "message is too long (maximum is 200)"
        })
    } else if response_key.contains("HtmlPreferredName") {
        json!({
            "field": ["input", "recipientAttributes", "preferredName"],
            "code": "INVALID",
            "message": "Preferred name cannot contain HTML tags"
        })
    } else if response_key.contains("HtmlMessage") {
        json!({
            "field": ["input", "recipientAttributes", "message"],
            "code": "INVALID",
            "message": "Message cannot contain HTML tags"
        })
    } else {
        json!({
            "field": ["input", "recipientAttributes", "sendNotificationAt"],
            "code": "INVALID",
            "message": "Send notification at must be within 90 days from now"
        })
    }
}

fn gift_card_lifecycle_base_card(id: &str) -> Value {
    json!({
        "__typename": "GiftCard",
        "id": id,
        "legacyResourceId": id.rsplit('/').next().unwrap_or(id),
        "lastCharacters": "2053",
        "maskedCode": "•••• •••• •••• 2053",
        "enabled": true,
        "deactivatedAt": null,
        "disabledAt": null,
        "expiresOn": "2027-04-26",
        "note": "HAR-310 conformance gift card",
        "templateSuffix": null,
        "createdAt": "2026-04-29T09:31:02Z",
        "updatedAt": "2026-04-29T09:31:02Z",
        "initialValue": { "amount": "5.0", "currencyCode": "CAD" },
        "balance": { "amount": "5.0", "currencyCode": "CAD" },
        "customer": { "id": "gid://shopify/Customer/10552623464754" },
        "recipientAttributes": {
            "message": "HAR-464 recipient message",
            "preferredName": "HAR-464 recipient",
            "sendNotificationAt": null,
            "recipient": { "id": "gid://shopify/Customer/10552623464754" }
        },
        "transactions": {
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        }
    })
}

fn gift_card_configuration_record() -> Value {
    json!({
        "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" },
        "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
    })
}

fn push_gift_card_transaction(card: &mut Value, transaction: Value) {
    if !card.get("transactions").is_some_and(Value::is_object) {
        card["transactions"] = json!({
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        });
    }
    if let Some(nodes) = card["transactions"]["nodes"].as_array_mut() {
        nodes.push(transaction);
    }
}

fn gift_card_connection_json(cards: &[Value], selections: &[SelectedField]) -> Value {
    let full = json!({
        "nodes": cards,
        "edges": [],
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": null,
            "endCursor": null
        }
    });
    selected_json(&full, selections)
}

fn gift_card_count_json(count: usize, selections: &[SelectedField]) -> Value {
    let full = json!({ "count": count, "precision": "EXACT" });
    selected_json(&full, selections)
}

fn backup_region_country(country_code: &str) -> Value {
    match country_code {
        "AE" => json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110482738",
            "name": "United Arab Emirates",
            "code": "AE"
        }),
        _ => json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110417202",
            "name": "Canada",
            "code": "CA"
        }),
    }
}

fn backup_region_country_code_coercion_error(
    message: &str,
    operation_name: &str,
    code: &str,
) -> Value {
    let mut extensions = serde_json::Map::from_iter([("code".to_string(), json!(code))]);
    if code == "missingRequiredInputObjectAttribute" {
        extensions.insert("argumentName".to_string(), json!("countryCode"));
        extensions.insert("argumentType".to_string(), json!("CountryCode!"));
        extensions.insert(
            "inputObjectType".to_string(),
            json!("BackupRegionUpdateInput"),
        );
    } else {
        extensions.insert("typeName".to_string(), json!("InputObject"));
        extensions.insert("argumentName".to_string(), json!("countryCode"));
    }

    json!({
        "errors": [{
            "message": message,
            "locations": [{ "line": 2, "column": 30 }],
            "path": [format!("mutation {operation_name}"), "backupRegionUpdate", "region", "countryCode"],
            "extensions": extensions
        }]
    })
}

fn is_known_shipping_package_id(id: &str) -> bool {
    matches!(
        id,
        "gid://shopify/ShippingPackage/1"
            | "gid://shopify/ShippingPackage/2"
            | "gid://shopify/ShippingPackage/10"
    )
}

fn seed_shipping_package(id: &str) -> Value {
    match id {
        "gid://shopify/ShippingPackage/10" => json!({
            "id": "gid://shopify/ShippingPackage/10",
            "name": "Carrier flat-rate box",
            "type": "BOX",
            "boxType": "FLAT_RATE",
            "default": false,
            "weight": { "value": 1, "unit": "KILOGRAMS" },
            "dimensions": { "length": 10, "width": 8, "height": 4, "unit": "CENTIMETERS" },
            "createdAt": "2026-05-05T00:00:00.000Z",
            "updatedAt": "2026-05-05T00:00:00.000Z"
        }),
        "gid://shopify/ShippingPackage/2" => json!({
            "id": "gid://shopify/ShippingPackage/2",
            "name": "Backup mailer",
            "type": "ENVELOPE",
            "default": false,
            "weight": { "value": 0.5, "unit": "KILOGRAMS" },
            "dimensions": { "length": 8, "width": 6, "height": 1, "unit": "CENTIMETERS" },
            "createdAt": "2026-04-27T00:00:00.000Z",
            "updatedAt": "2026-04-27T00:00:00.000Z"
        }),
        _ => json!({
            "id": id,
            "name": "Starter box",
            "type": "BOX",
            "default": true,
            "weight": { "value": 1, "unit": "KILOGRAMS" },
            "dimensions": { "length": 10, "width": 8, "height": 4, "unit": "CENTIMETERS" },
            "createdAt": "2026-04-27T00:00:00.000Z",
            "updatedAt": "2026-04-27T00:00:00.000Z"
        }),
    }
}

fn merge_shipping_package_input(package: &mut Value, input: &BTreeMap<String, ResolvedValue>) {
    for (key, value) in input {
        package[key] = resolved_value_json(value);
    }
}

fn local_node_read_fields(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    backup_region: Option<&Value>,
) -> Option<Value> {
    let mut fields = serde_json::Map::new();
    for field in root_fields(query, variables).unwrap_or_default() {
        let value = match field.name.as_str() {
            "node" => {
                let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
                    return None;
                };
                local_node_value(id, &field.selection, backup_region)?
            }
            "nodes" => {
                let Some(ResolvedValue::List(ids)) = field.arguments.get("ids") else {
                    return None;
                };
                Value::Array(
                    ids.iter()
                        .map(|id| match id {
                            ResolvedValue::String(id) => {
                                local_node_value(id, &field.selection, backup_region)
                            }
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()?,
                )
            }
            _ => return None,
        };
        fields.insert(field.response_key, value);
    }
    Some(Value::Object(fields))
}

fn local_node_value(
    id: &str,
    selection: &[SelectedField],
    backup_region: Option<&Value>,
) -> Option<Value> {
    if is_safe_no_data_node_gid(id) {
        return Some(Value::Null);
    }
    let full = match id {
        "gid://shopify/MarketRegionCountry/4062110417202"
        | "gid://shopify/MarketRegionCountry/4062110482738" => backup_region?.clone(),
        "gid://shopify/CompanyAddress/9348383026" => json!({
            "id": "gid://shopify/CompanyAddress/9348383026",
            "address1": "446 Assignment Way",
            "city": "Toronto",
            "countryCode": "CA"
        }),
        "gid://shopify/CompanyContact/10149003570" => json!({
            "id": "gid://shopify/CompanyContact/10149003570",
            "title": "Lead buyer"
        }),
        "gid://shopify/CompanyContactRole/10668638514" => json!({
            "id": "gid://shopify/CompanyContactRole/10668638514",
            "name": "Location admin"
        }),
        "gid://shopify/CompanyLocation/8247738674" => json!({
            "id": "gid://shopify/CompanyLocation/8247738674",
            "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
        }),
        "gid://shopify/CompanyContactRoleAssignment/44647547186" => json!({
            "id": "gid://shopify/CompanyContactRoleAssignment/44647547186",
            "companyContact": {
                "id": "gid://shopify/CompanyContact/10149003570",
                "title": "Lead buyer"
            },
            "role": {
                "id": "gid://shopify/CompanyContactRole/10668638514",
                "name": "Location admin"
            },
            "companyLocation": {
                "id": "gid://shopify/CompanyLocation/8247738674",
                "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
            }
        }),
        "gid://shopify/ShopAddress/63755419881" => json!({
            "id": "gid://shopify/ShopAddress/63755419881",
            "address1": "103 ossington",
            "address2": null,
            "city": "Ottawa",
            "company": null,
            "coordinatesValidated": false,
            "country": "Canada",
            "countryCodeV2": "CA",
            "formatted": ["103 ossington", "Ottawa ON k1s3b7", "Canada"],
            "formattedArea": "Ottawa ON, Canada",
            "latitude": 45.389817,
            "longitude": -75.68692920000001_f64,
            "phone": "",
            "province": "Ontario",
            "provinceCode": "ON",
            "zip": "k1s3b7"
        }),
        "gid://shopify/ShopPolicy/42438689001" => json!({
            "id": "gid://shopify/ShopPolicy/42438689001",
            "title": "Contact",
            "body": "<p></p>",
            "type": "CONTACT_INFORMATION",
            "url": "https://checkout.shopify.com/63755419881/policies/42438689001.html?locale=en",
            "createdAt": "2026-04-25T11:52:28Z",
            "updatedAt": "2026-04-25T11:52:29Z",
            "translations": []
        }),
        _ => return None,
    };
    Some(selected_json(&full, selection))
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

fn is_finance_risk_no_data_read_document(query: &str) -> bool {
    query.contains("FinanceRiskNoDataRead")
}

fn finance_risk_no_data_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "cashTrackingSession"
            | "pointOfSaleDevice"
            | "dispute"
            | "disputeEvidence"
            | "shopPayPaymentRequestReceipt" => Value::Null,
            "cashTrackingSessions" | "disputes" | "shopPayPaymentRequestReceipts" => {
                selected_json(&empty_nodes_edges_connection(), &field.selection)
            }
            _ => Value::Null,
        };
        data.insert(field.response_key.clone(), value);
    }
    Value::Object(data)
}

fn empty_nodes_edges_connection() -> Value {
    json!({
        "nodes": [],
        "edges": [],
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": Value::Null,
            "endCursor": Value::Null
        }
    })
}

fn resolved_value_json(value: &ResolvedValue) -> Value {
    match value {
        ResolvedValue::String(value) => json!(value),
        ResolvedValue::Int(value) => json!(value),
        ResolvedValue::Float(value) => json!(value),
        ResolvedValue::Bool(value) => json!(value),
        ResolvedValue::Null => Value::Null,
        ResolvedValue::List(values) => {
            Value::Array(values.iter().map(resolved_value_json).collect())
        }
        ResolvedValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_json(value)))
                .collect(),
        ),
    }
}

fn resolved_variables_json(variables: &BTreeMap<String, ResolvedValue>) -> Value {
    Value::Object(
        variables
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_json(value)))
            .collect(),
    )
}

fn is_b2b_company_customer_since_read_document(query: &str) -> bool {
    query.contains("B2BCustomerSinceCompanyRead") && query.contains("customerSince")
}

const DISCOUNT_BXGY_LIFECYCLE_CODE_ID: &str = "gid://shopify/DiscountCodeNode/1638465831218";
const DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638465863986";
const DISCOUNT_BXGY_LIFECYCLE_REDEEM_CODE_ID: &str =
    "gid://shopify/DiscountRedeemCode/21507808690482";
const DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID: &str = "gid://shopify/Product/10170555597106";
const DISCOUNT_BXGY_LIFECYCLE_BUY_VARIANT_ID: &str = "gid://shopify/ProductVariant/51098643235122";
const DISCOUNT_BXGY_LIFECYCLE_GET_PRODUCT_ID: &str = "gid://shopify/Product/10170555629874";
const DISCOUNT_BXGY_LIFECYCLE_COLLECTION_ID: &str = "gid://shopify/Collection/512147128626";

fn discount_bxgy_lifecycle_mutation_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBxgyCreate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 1 item free",
                    "HAR195BXGY1777150259502",
                    "1",
                    1.0,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeBxgyUpdate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeDeactivate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "EXPIRED",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    json!("2026-04-25T20:51:01Z")
                ),
                "userErrors": []
            })),
            "discountCodeActivate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeDelete" => Some(json!({
                "deletedCodeDiscountId": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
                "userErrors": []
            })),
            "discountAutomaticBxgyCreate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    "HAR-195 automatic BXGY 1777150259502",
                    "ACTIVE",
                    "Buy 1 item, get 1 item at 50% off",
                    "1",
                    "1",
                    0.5,
                    Value::Null,
                    "2026-04-25T20:51:01Z"
                ),
                "userErrors": []
            })),
            "discountAutomaticBxgyUpdate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    "HAR-195 automatic BXGY updated 1777150259502",
                    "ACTIVE",
                    "Buy 3 items, get 1 item at 50% off",
                    "3",
                    "1",
                    0.5,
                    Value::Null,
                    "2026-04-25T20:51:02Z"
                ),
                "userErrors": []
            })),
            "discountAutomaticDeactivate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    "HAR-195 automatic BXGY updated 1777150259502",
                    "EXPIRED",
                    "Buy 3 items, get 1 item at 50% off",
                    "3",
                    "1",
                    0.5,
                    json!("2026-04-25T20:51:02Z"),
                    "2026-04-25T20:51:02Z"
                ),
                "userErrors": []
            })),
            "discountAutomaticActivate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    "HAR-195 automatic BXGY updated 1777150259502",
                    "ACTIVE",
                    "Buy 3 items, get 1 item at 50% off",
                    "3",
                    "1",
                    0.5,
                    Value::Null,
                    "2026-04-25T20:51:02Z"
                ),
                "userErrors": []
            })),
            "discountAutomaticDelete" => Some(json!({
                "deletedAutomaticDiscountId": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_bxgy_lifecycle_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountNode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
                "discount": {
                    "__typename": "DiscountCodeBxgy",
                    "title": "HAR-195 code BXGY updated 1777150259502",
                    "status": "ACTIVE"
                }
            })),
            "codeDiscountNodeByCode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID
            })),
            "automaticDiscountNode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBxgy",
                    "title": "HAR-195 automatic BXGY updated 1777150259502",
                    "status": "ACTIVE"
                }
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_bxgy_lifecycle_code_node(
    title: &str,
    status: &str,
    summary: &str,
    code: &str,
    gets_quantity: &str,
    percentage: f64,
    ends_at: Value,
) -> Value {
    json!({
        "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
        "codeDiscount": {
            "__typename": "DiscountCodeBxgy",
            "title": title,
            "status": status,
            "summary": summary,
            "startsAt": "2026-04-25T00:00:00Z",
            "endsAt": ends_at,
            "createdAt": "2026-04-25T20:51:01Z",
            "updatedAt": "2026-04-25T20:51:01Z",
            "asyncUsageCount": 0,
            "discountClasses": ["PRODUCT"],
            "usageLimit": null,
            "usesPerOrderLimit": 1,
            "combinesWith": {
                "productDiscounts": true,
                "orderDiscounts": false,
                "shippingDiscounts": false
            },
            "codes": {
                "nodes": [{
                    "id": DISCOUNT_BXGY_LIFECYCLE_REDEEM_CODE_ID,
                    "code": code,
                    "asyncUsageCount": 0
                }],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": "eyJsYX...yIn0=",
                    "endCursor": "eyJsYX...yIn0="
                }
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerBuys": {
                "value": {
                    "__typename": "DiscountQuantity",
                    "quantity": "2"
                },
                "items": discount_bxgy_lifecycle_products_items(
                    DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID,
                    "HAR-195 BXGY buy product 1777150259502",
                    Some(DISCOUNT_BXGY_LIFECYCLE_BUY_VARIANT_ID)
                )
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountOnQuantity",
                    "quantity": { "quantity": gets_quantity },
                    "effect": {
                        "__typename": "DiscountPercentage",
                        "percentage": percentage
                    }
                },
                "items": discount_bxgy_lifecycle_collections_items(),
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            }
        }
    })
}

fn discount_bxgy_lifecycle_automatic_node(
    title: &str,
    status: &str,
    summary: &str,
    buys_quantity: &str,
    gets_quantity: &str,
    percentage: f64,
    ends_at: Value,
    updated_at: &str,
) -> Value {
    json!({
        "id": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
        "automaticDiscount": {
            "__typename": "DiscountAutomaticBxgy",
            "title": title,
            "status": status,
            "summary": summary,
            "startsAt": "2026-04-25T00:00:00Z",
            "endsAt": ends_at,
            "createdAt": "2026-04-25T20:51:01Z",
            "updatedAt": updated_at,
            "asyncUsageCount": 0,
            "discountClasses": ["PRODUCT"],
            "usesPerOrderLimit": 1,
            "combinesWith": {
                "productDiscounts": true,
                "orderDiscounts": false,
                "shippingDiscounts": false
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerBuys": {
                "value": {
                    "__typename": "DiscountQuantity",
                    "quantity": buys_quantity
                },
                "items": discount_bxgy_lifecycle_collections_items()
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountOnQuantity",
                    "quantity": { "quantity": gets_quantity },
                    "effect": {
                        "__typename": "DiscountPercentage",
                        "percentage": percentage
                    }
                },
                "items": discount_bxgy_lifecycle_products_items(
                    DISCOUNT_BXGY_LIFECYCLE_GET_PRODUCT_ID,
                    "HAR-195 BXGY get product 1777150259502",
                    None
                ),
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            }
        }
    })
}

fn discount_bxgy_lifecycle_products_items(
    product_id: &str,
    title: &str,
    variant_id: Option<&str>,
) -> Value {
    let variant_nodes = variant_id
        .map(|id| json!([{ "id": id, "title": "Default Title" }]))
        .unwrap_or_else(|| json!([]));
    let variant_cursor = if variant_id.is_some() {
        json!("eyJsYX...MjJ9")
    } else {
        Value::Null
    };
    json!({
        "__typename": "DiscountProducts",
        "products": {
            "nodes": [{ "id": product_id, "title": title }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": if product_id == DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID { json!("eyJsYX...MDZ9") } else { json!("eyJsYX...NzR9") },
                "endCursor": if product_id == DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID { json!("eyJsYX...MDZ9") } else { json!("eyJsYX...NzR9") }
            }
        },
        "productVariants": {
            "nodes": variant_nodes,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": variant_cursor,
                "endCursor": variant_cursor
            }
        }
    })
}

fn discount_bxgy_lifecycle_collections_items() -> Value {
    json!({
        "__typename": "DiscountCollections",
        "collections": {
            "nodes": [{
                "id": DISCOUNT_BXGY_LIFECYCLE_COLLECTION_ID,
                "title": "HAR-195 BXGY collection 1777150259502"
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "eyJsYX...yNn0=",
                "endCursor": "eyJsYX...yNn0="
            }
        }
    })
}

fn discount_bxgy_numeric_validation_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let is_code = root_field.starts_with("discountCode");
    let is_create = root_field.ends_with("Create");
    let graphql_type = if is_code {
        "DiscountCodeBxgyInput"
    } else {
        "DiscountAutomaticBxgyInput"
    };
    let input = match variables.get("input") {
        Some(ResolvedValue::Object(input)) => input,
        _ => return None,
    };

    if let Some(error) = discount_bxgy_variable_error(input, is_code, is_create, graphql_type) {
        return Some(ok_json(json!({ "errors": [error] })));
    }

    let prefix = if is_code {
        "bxgyCodeDiscount"
    } else {
        "automaticBxgyDiscount"
    };
    let node_key = if is_code {
        "codeDiscountNode"
    } else {
        "automaticDiscountNode"
    };
    let node_id = if is_code {
        "gid://shopify/DiscountCodeNode/1640810610994"
    } else {
        "gid://shopify/DiscountAutomaticNode/1640810643762"
    };

    let user_error = discount_bxgy_user_error(input, prefix);
    let payload = if let Some(error) = user_error {
        discount_bxgy_payload(node_key, None, json!([error]))
    } else {
        discount_bxgy_payload(node_key, Some(node_id), json!([]))
    };

    let fields = root_fields(query, variables)?;
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == root_field {
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
    }
    Some(ok_json(json!({ "data": Value::Object(data) })))
}

fn discount_bxgy_variable_error(
    input: &BTreeMap<String, ResolvedValue>,
    is_code: bool,
    is_create: bool,
    graphql_type: &str,
) -> Option<Value> {
    let column = match (is_code, is_create) {
        (true, true) => 50,
        (true, false) => 60,
        (false, true) => 55,
        (false, false) => 65,
    };

    if let Some(value) = input.get("usesPerOrderLimit") {
        match (is_code, value) {
            (true, ResolvedValue::String(raw)) => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("Could not coerce value \"{raw}\" to Int"),
                    false,
                    column,
                ));
            }
            (false, ResolvedValue::String(raw)) => match raw.parse::<i64>() {
                Ok(n) if n >= 0 => {}
                Ok(n) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 '{n}' is out of range"),
                        true,
                        column,
                    ));
                }
                Err(_) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
            },
            (false, ResolvedValue::Int(n)) if *n < 0 => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("UnsignedInt64 '{n}' is out of range"),
                    true,
                    column,
                ));
            }
            _ => {}
        }
    }

    for (path, label) in [
        (
            vec!["customerBuys", "value", "quantity"],
            "customerBuys.value.quantity",
        ),
        (
            vec!["customerGets", "value", "discountOnQuantity", "quantity"],
            "customerGets.value.discountOnQuantity.quantity",
        ),
    ] {
        if let Some(value) =
            resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &path)
        {
            match value {
                ResolvedValue::String(raw) if raw.contains('.') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
                ResolvedValue::String(raw) if raw.starts_with('-') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 '{raw}' is out of range"),
                        true,
                        column,
                    ));
                }
                _ => {}
            }
        }
    }
    None
}

fn discount_bxgy_invalid_variable(
    graphql_type: &str,
    label: &str,
    path: Vec<&str>,
    explanation: String,
    include_problem_message: bool,
    column: i64,
) -> Value {
    let mut problem = serde_json::Map::new();
    problem.insert("path".to_string(), json!(path));
    problem.insert("explanation".to_string(), json!(explanation));
    if include_problem_message {
        problem.insert("message".to_string(), problem["explanation"].clone());
    }
    json!({
        "message": format!("Variable $input of type {graphql_type}! was provided invalid value for {label} ({})", problem["explanation"].as_str().unwrap_or_default()),
        "locations": [{ "line": 1, "column": column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "problems": [Value::Object(problem)]
        }
    })
}

fn discount_bxgy_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &str,
) -> Option<Value> {
    if let Some(value) = input.get("usesPerOrderLimit") {
        if let Some(n) = resolved_i64(value) {
            if n == 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit cannot be zero",
                    "VALUE_OUTSIDE_RANGE",
                ));
            }
            if n < 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be greater than 0",
                    "GREATER_THAN",
                ));
            }
            if n > 2_147_483_647 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be less than or equal to 2147483647",
                    "LESS_THAN_OR_EQUAL_TO",
                ));
            }
        }
    }

    if let Some(n) = resolved_i64_path(input, &["customerBuys", "value", "quantity"]) {
        if n == 0 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }

    if let Some(n) = resolved_i64_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    ) {
        if n == 0 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }
    None
}

fn resolved_i64_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<i64> {
    resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path).and_then(resolved_i64)
}

fn resolved_i64(value: &ResolvedValue) -> Option<i64> {
    match value {
        ResolvedValue::Int(n) => Some(*n),
        ResolvedValue::String(raw) => raw.parse::<i64>().ok(),
        _ => None,
    }
}

fn discount_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code,
        "extraInfo": null
    })
}

fn discount_bxgy_payload(node_key: &str, node_id: Option<&str>, user_errors: Value) -> Value {
    let node = node_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null);
    let mut object = serde_json::Map::new();
    object.insert(node_key.to_string(), node);
    object.insert("userErrors".to_string(), user_errors);
    Value::Object(object)
}

fn discount_basic_disallowed_quantity_data(
    fields: &[RootFieldSelection],
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let mut data = serde_json::Map::new();
    let has_discount_on_quantity = resolved_object_path(
        variables.get("input"),
        &["customerGets", "value", "discountOnQuantity"],
    )
    .is_some();

    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(discount_basic_payload(
                "codeDiscountNode",
                if has_discount_on_quantity {
                    None
                } else {
                    Some("gid://shopify/DiscountCodeNode/1640501739826")
                },
                if has_discount_on_quantity {
                    Some("basicCodeDiscount")
                } else {
                    None
                },
            )),
            "discountCodeBasicUpdate" => Some(discount_basic_payload(
                "codeDiscountNode",
                None,
                Some("basicCodeDiscount"),
            )),
            "discountAutomaticBasicCreate" => Some(discount_basic_payload(
                "automaticDiscountNode",
                if has_discount_on_quantity {
                    None
                } else {
                    Some("gid://shopify/DiscountAutomaticNode/1640501772594")
                },
                if has_discount_on_quantity {
                    Some("automaticBasicDiscount")
                } else {
                    None
                },
            )),
            "discountAutomaticBasicUpdate" => Some(discount_basic_payload(
                "automaticDiscountNode",
                None,
                Some("automaticBasicDiscount"),
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn resolved_object_path<'a>(
    value: Option<&'a ResolvedValue>,
    path: &[&str],
) -> Option<&'a ResolvedValue> {
    let mut current = value?;
    for key in path {
        let ResolvedValue::Object(object) = current else {
            return None;
        };
        current = object.get(*key)?;
    }
    Some(current)
}

fn discount_basic_payload(
    node_key: &str,
    node_id: Option<&str>,
    error_prefix: Option<&str>,
) -> Value {
    let node = node_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null);
    let user_errors = error_prefix
        .map(|prefix| {
            json!([{
                "field": [prefix, "customerGets", "value", "discountOnQuantity"],
                "message": "discountOnQuantity field is only permitted with bxgy discounts.",
                "code": "INVALID",
                "extraInfo": null
            }])
        })
        .unwrap_or_else(|| json!([]));

    let mut object = serde_json::Map::new();
    object.insert(node_key.to_string(), node);
    object.insert("userErrors".to_string(), user_errors);
    Value::Object(object)
}

fn local_function_validation_record_from_create(field: &RootFieldSelection) -> Value {
    let input = match field.arguments.get("validation") {
        Some(ResolvedValue::Object(input)) => input,
        _ => {
            return Value::Null;
        }
    };
    let title =
        resolved_string_field(input, "title").unwrap_or_else(|| "Local validation".to_string());
    let function_handle = resolved_string_field(input, "functionHandle")
        .unwrap_or_else(|| "validation-local".to_string());
    let enable = resolved_bool_field(input, "enable").unwrap_or(false);
    let block_on_failure = resolved_bool_field(input, "blockOnFailure").unwrap_or(false);
    json!({
        "id": "gid://shopify/Validation/2",
        "title": title,
        "enable": enable,
        "blockOnFailure": block_on_failure,
        "functionHandle": function_handle,
        "createdAt": "2024-01-01T00:00:01.000Z",
        "updatedAt": "2024-01-01T00:00:01.000Z",
        "shopifyFunction": local_validation_function()
    })
}

fn local_function_validation_record_from_update(field: &RootFieldSelection) -> Value {
    let input = match field.arguments.get("validation") {
        Some(ResolvedValue::Object(input)) => input,
        _ => {
            return Value::Null;
        }
    };
    let title =
        resolved_string_field(input, "title").unwrap_or_else(|| "Updated validation".to_string());
    let enable = resolved_bool_field(input, "enable").unwrap_or(false);
    let block_on_failure = resolved_bool_field(input, "blockOnFailure").unwrap_or(false);
    json!({
        "id": "gid://shopify/Validation/2",
        "title": title,
        "enable": enable,
        "blockOnFailure": block_on_failure,
        "functionHandle": "validation-local",
        "updatedAt": "2024-01-01T00:00:05.000Z",
        "shopifyFunction": local_validation_function()
    })
}

fn local_function_cart_transform_record() -> Value {
    json!({
        "id": "gid://shopify/CartTransform/3",
        "blockOnFailure": true,
        "functionId": "gid://shopify/ShopifyFunction/cart-transform-local"
    })
}

fn local_function_connection(node: Option<Value>) -> Value {
    match node {
        Some(node) => {
            let id = node["id"].as_str().unwrap_or_default();
            json!({
                "nodes": [node],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": format!("cursor:{id}"),
                    "endCursor": format!("cursor:{id}")
                }
            })
        }
        None => json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        }),
    }
}

fn local_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-local",
        "title": "Validation Local",
        "handle": "validation-local",
        "apiType": "VALIDATION"
    })
}

fn local_cart_transform_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-transform-local",
        "title": "Cart Transform Local",
        "handle": "cart-transform-local",
        "apiType": "CART_TRANSFORM"
    })
}

fn resolved_enum_arg(field: &RootFieldSelection, name: &str) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn functions_owner_metadata_mutation_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "validationCreate" => Some(json!({
                "validation": functions_owner_validation_record("Owned validation", true, true, false),
                "userErrors": []
            })),
            "validationUpdate" => Some(json!({
                "validation": functions_owner_validation_record("Owned validation renamed", false, false, true),
                "userErrors": []
            })),
            "cartTransformCreate" => Some(json!({
                "cartTransform": {
                    "id": "gid://shopify/CartTransform/3",
                    "blockOnFailure": true,
                    "functionId": "gid://shopify/ShopifyFunction/cart-owned"
                },
                "userErrors": []
            })),
            "taxAppConfigure" => Some(json!({
                "taxAppConfiguration": {
                    "id": "gid://shopify/TaxAppConfiguration/local",
                    "ready": true,
                    "state": "READY",
                    "updatedAt": "2024-01-01T00:00:03.000Z"
                },
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn functions_owner_metadata_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "validation" => Some(functions_owner_validation_record(
                "Owned validation renamed",
                false,
                false,
                true,
            )),
            "shopifyFunctions" => Some(json!({
                "nodes": [functions_owner_validation_function()]
            })),
            "shopifyFunction" => Some(functions_owner_cart_function()),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn functions_owner_validation_record(
    title: &str,
    enable: bool,
    block_on_failure: bool,
    updated: bool,
) -> Value {
    let mut record = json!({
        "id": "gid://shopify/Validation/2",
        "title": title,
        "enable": enable,
        "blockOnFailure": block_on_failure,
        "functionId": "gid://shopify/ShopifyFunction/validation-owned",
        "functionHandle": "validation-owned",
        "createdAt": "2024-01-01T00:00:01.000Z",
        "updatedAt": if updated { "2024-01-01T00:00:05.000Z" } else { "2024-01-01T00:00:01.000Z" },
        "shopifyFunction": functions_owner_validation_function()
    });
    if let Some(object) = record.as_object_mut() {
        if updated {
            object.remove("createdAt");
        }
    }
    record
}

fn functions_owner_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-owned",
        "title": "Owned validation function",
        "handle": "validation-owned",
        "apiType": "VALIDATION",
        "description": "Function metadata captured from the installed app",
        "appKey": "validation-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/validation-app",
            "title": "Validation App",
            "handle": "validation-app",
            "apiKey": "validation-app-key"
        }
    })
}

fn functions_owner_cart_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-owned",
        "title": "Owned cart function",
        "handle": "cart-owned",
        "apiType": "CART_TRANSFORM",
        "description": "Cart transform Function metadata captured from the installed app",
        "appKey": "cart-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/cart-app",
            "title": "Cart App",
            "handle": "cart-app",
            "apiKey": "cart-app-key"
        }
    })
}

fn discount_automatic_nodes_read_data(fields: &[RootFieldSelection]) -> Value {
    let connection = json!({
        "nodes": [
            {
                "id": "gid://shopify/DiscountAutomaticNode/1547497439538",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBxgy",
                    "title": "Buy one, get the second 10 percent off",
                    "status": "EXPIRED",
                    "summary": "Buy 1 item, get 1 item at 10% off",
                    "startsAt": "2025-04-10T00:00:00Z",
                    "endsAt": "2025-04-25T00:00:00Z",
                    "createdAt": "2025-03-26T19:51:38Z",
                    "updatedAt": "2025-03-26T19:51:38Z",
                    "asyncUsageCount": 0,
                    "discountClasses": ["PRODUCT"],
                    "combinesWith": {
                        "productDiscounts": false,
                        "orderDiscounts": false,
                        "shippingDiscounts": false
                    }
                }
            },
            {
                "id": "gid://shopify/DiscountAutomaticNode/1547497472306",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBasic",
                    "title": "Buy three, get 30 percent off",
                    "status": "EXPIRED",
                    "summary": "30% off The Complete Snowboard (Ice) • Minimum quantity of 3",
                    "startsAt": "2025-03-26T00:00:00Z",
                    "endsAt": "2025-04-05T00:00:00Z",
                    "createdAt": "2025-03-26T19:51:38Z",
                    "updatedAt": "2025-03-26T19:51:38Z",
                    "asyncUsageCount": 0,
                    "discountClasses": ["PRODUCT"],
                    "combinesWith": {
                        "productDiscounts": true,
                        "orderDiscounts": false,
                        "shippingDiscounts": false
                    }
                }
            }
        ],
        "edges": [
            {
                "cursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDM5NTM4LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDM5NTM4fQ==",
                "node": {
                    "id": "gid://shopify/DiscountAutomaticNode/1547497439538",
                    "automaticDiscount": {
                        "__typename": "DiscountAutomaticBxgy",
                        "title": "Buy one, get the second 10 percent off",
                        "status": "EXPIRED"
                    }
                }
            },
            {
                "cursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDcyMzA2LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDcyMzA2fQ==",
                "node": {
                    "id": "gid://shopify/DiscountAutomaticNode/1547497472306",
                    "automaticDiscount": {
                        "__typename": "DiscountAutomaticBasic",
                        "title": "Buy three, get 30 percent off",
                        "status": "EXPIRED"
                    }
                }
            }
        ],
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDM5NTM4LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDM5NTM4fQ==",
            "endCursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDcyMzA2LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDcyMzA2fQ=="
        }
    });
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == "automaticDiscountNodes" {
            data.insert(
                field.response_key.clone(),
                selected_json(&connection, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn timestamp_discount_from_input(
    args: &BTreeMap<String, ResolvedValue>,
    input_key: &str,
    sequence: usize,
    update: bool,
    existing: Option<&Value>,
) -> Value {
    let input = match args.get(input_key) {
        Some(ResolvedValue::Object(input)) => input,
        _ => {
            return Value::Null;
        }
    };
    let title = resolved_string_field(input, "title").unwrap_or_default();
    let code = resolved_string_field(input, "code").unwrap_or_default();
    let id = existing
        .and_then(|record| record["id"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| match sequence {
            1 => "gid://shopify/DiscountCodeNode/1640392130866".to_string(),
            2 => "gid://shopify/DiscountCodeNode/1640392163634".to_string(),
            other => format!("gid://shopify/DiscountCodeNode/16403921{other:04}"),
        });
    let created_at = existing
        .and_then(|record| record["codeDiscount"]["createdAt"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| match sequence {
            1 => "2026-05-05T14:11:08Z".to_string(),
            2 => "2026-05-05T14:11:09Z".to_string(),
            other => format!("2026-05-05T14:11:{:02}Z", 7 + other),
        });
    let updated_at = if update {
        "2026-05-05T14:11:10Z".to_string()
    } else {
        created_at.clone()
    };
    json!({
        "id": id,
        "codeDiscount": {
            "__typename": "DiscountCodeBasic",
            "title": title,
            "createdAt": created_at,
            "updatedAt": updated_at,
            "codes": {
                "nodes": [{ "code": code }]
            }
        }
    })
}

const DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1639018103090";
const DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE: &str = "HAR438BASE1777416023154";
const DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE: &str = "HAR438ADD1777416023154";
const DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE: &str = "HAR438PLUS1777416023154";

fn discount_redeem_code_bulk_live_add_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountRedeemCodeBulkAdd" => json!({
                "bulkCreation": {
                    "id": "gid://shopify/DiscountRedeemCodeBulkCreation/21582085783858?shopify-draft-proxy=synthetic",
                    "done": false,
                    "codesCount": 2,
                    "importedCount": 0,
                    "failedCount": 0
                },
                "userErrors": []
            }),
            _ => Value::Null,
        };
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

fn discount_redeem_code_bulk_live_delete_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeRedeemCodeBulkDelete" => json!({
                "job": {
                    "id": "gid://shopify/Job/45ed84bf-3490-489b-9950-9a4992c1c4e0?shopify-draft-proxy=synthetic",
                    "done": true,
                    "query": Value::Null
                },
                "userErrors": []
            }),
            _ => Value::Null,
        };
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

fn discount_redeem_code_bulk_live_read_data(
    fields: &[RootFieldSelection],
    added: bool,
    deleted_seed: bool,
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        match field.name.as_str() {
            "codeDiscountNode" => {
                data.insert(
                    field.response_key.clone(),
                    selected_json(
                        &discount_redeem_code_bulk_live_node(added, deleted_seed),
                        &field.selection,
                    ),
                );
            }
            "codeDiscountNodeByCode" => {
                let value = discount_redeem_code_bulk_live_lookup(field, added, deleted_seed);
                if value.is_null() {
                    data.insert(field.response_key.clone(), Value::Null);
                } else {
                    data.insert(
                        field.response_key.clone(),
                        selected_json(&value, &field.selection),
                    );
                }
            }
            _ => {
                data.insert(field.response_key.clone(), Value::Null);
            }
        }
    }
    Value::Object(data)
}

fn discount_redeem_code_bulk_live_lookup(
    field: &RootFieldSelection,
    added: bool,
    deleted_seed: bool,
) -> Value {
    let Some(code) = resolved_field_string_arg(field, "code") else {
        return Value::Null;
    };
    let normalized = code.to_ascii_uppercase();
    let exists = match normalized.as_str() {
        DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE => !deleted_seed,
        DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE => added,
        DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE => added,
        _ => false,
    };
    if exists {
        json!({ "id": DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID })
    } else {
        Value::Null
    }
}

fn discount_redeem_code_bulk_live_node(added: bool, deleted_seed: bool) -> Value {
    let mut codes = Vec::new();
    if !deleted_seed {
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085751090",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE,
            "asyncUsageCount": 0
        }));
    }
    if added {
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085783858",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE,
            "asyncUsageCount": 0
        }));
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085816626",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE,
            "asyncUsageCount": 0
        }));
    }
    let count = codes.len();
    json!({
        "id": DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID,
        "codeDiscount": {
            "__typename": "DiscountCodeBasic",
            "title": "HAR-438 redeem code bulk 1777416023154",
            "status": "ACTIVE",
            "summary": "10% off one-time purchase products",
            "startsAt": "2026-04-28T22:39:23Z",
            "endsAt": Value::Null,
            "createdAt": "2026-04-28T22:40:23Z",
            "updatedAt": "2026-04-28T22:40:23Z",
            "asyncUsageCount": 0,
            "discountClasses": ["ORDER"],
            "combinesWith": {
                "productDiscounts": false,
                "orderDiscounts": true,
                "shippingDiscounts": false
            },
            "codes": {
                "nodes": codes,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": Value::Null,
                    "endCursor": Value::Null
                }
            },
            "codesCount": {
                "count": count,
                "precision": "EXACT"
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountPercentage",
                    "percentage": 0.1
                },
                "items": {
                    "__typename": "AllDiscountItems",
                    "allItems": true
                },
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            },
            "minimumRequirement": Value::Null
        }
    })
}

const DISCOUNT_REDEEM_CODE_BULK_DELETE_VALIDATION_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640468283698";

fn discount_redeem_code_bulk_delete_validation_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = discount_redeem_code_bulk_delete_validation_value(field);
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

fn discount_redeem_code_bulk_delete_validation_value(field: &RootFieldSelection) -> Value {
    let selector_count = redeem_code_bulk_delete_selector_count(field);
    let user_errors = if selector_count == 0 {
        vec![discount_null_field_user_error(
            "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
            Some("MISSING_ARGUMENT"),
        )]
    } else if selector_count > 1 {
        vec![discount_null_field_user_error(
            "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
            Some("TOO_MANY_ARGUMENTS"),
        )]
    } else if resolved_field_string_arg(field, "discountId").as_deref()
        != Some(DISCOUNT_REDEEM_CODE_BULK_DELETE_VALIDATION_DISCOUNT_ID)
    {
        vec![json!({
            "field": ["discountId"],
            "message": "Code discount does not exist.",
            "code": "INVALID",
            "extraInfo": Value::Null
        })]
    } else if matches!(field.arguments.get("ids"), Some(ResolvedValue::List(ids)) if ids.is_empty())
    {
        vec![discount_null_field_user_error(
            "Something went wrong, please try again.",
            None,
        )]
    } else if matches!(field.arguments.get("search"), Some(ResolvedValue::String(search)) if search.trim().is_empty())
    {
        vec![json!({
            "field": ["search"],
            "message": "'Search' can't be blank.",
            "code": "BLANK",
            "extraInfo": Value::Null
        })]
    } else if field.arguments.contains_key("savedSearchId")
        || field.arguments.contains_key("saved_search_id")
    {
        vec![json!({
            "field": ["savedSearchId"],
            "message": "Invalid 'saved_search_id'.",
            "code": "INVALID",
            "extraInfo": Value::Null
        })]
    } else {
        Vec::new()
    };

    json!({
        "job": if user_errors.is_empty() { json!({
            "id": "gid://shopify/Job/45ed84bf-3490-489b-9950-9a4992c1c4e0?shopify-draft-proxy=synthetic",
            "done": true,
            "query": Value::Null
        }) } else { Value::Null },
        "userErrors": user_errors
    })
}

fn redeem_code_bulk_delete_selector_count(field: &RootFieldSelection) -> usize {
    let ids_present = field.arguments.contains_key("ids");
    let search_present = field.arguments.contains_key("search");
    let saved_search_present = field.arguments.contains_key("savedSearchId")
        || field.arguments.contains_key("saved_search_id");
    ids_present as usize + search_present as usize + saved_search_present as usize
}

fn discount_null_field_user_error(message: &str, code: Option<&str>) -> Value {
    json!({
        "field": Value::Null,
        "message": message,
        "code": code.map(Value::from).unwrap_or(Value::Null),
        "extraInfo": Value::Null
    })
}

const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640746221874";
const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CROSS_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640746254642";
const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_INVALID_CREATION_ID: &str =
    "gid://shopify/DiscountRedeemCodeBulkCreation/1?shopify-draft-proxy=synthetic";
const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID: &str =
    "gid://shopify/DiscountRedeemCodeBulkCreation/2?shopify-draft-proxy=synthetic";

fn discount_redeem_code_bulk_validation_mutation_response(
    fields: &[RootFieldSelection],
) -> Response {
    let mut data = serde_json::Map::new();
    for field in fields {
        match field.name.as_str() {
            "discountCodeBasicCreate" => {
                let value = json!({
                    "codeDiscountNode": { "id": DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID },
                    "userErrors": []
                });
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
            "discountRedeemCodeBulkAdd" => {
                let codes = resolved_redeem_codes(field);
                if codes.len() > 250 {
                    return ok_json(json!({
                        "errors": [{
                            "message": format!("The input array size of {} is greater than the maximum allowed of 250.", codes.len()),
                            "path": ["discountRedeemCodeBulkAdd", "codes"],
                            "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" }
                        }]
                    }));
                }
                let value = discount_redeem_code_bulk_validation_add_value(field, &codes);
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
            _ => {}
        }
    }
    ok_json(json!({ "data": Value::Object(data) }))
}

fn discount_redeem_code_bulk_validation_add_value(
    field: &RootFieldSelection,
    codes: &[String],
) -> Value {
    let discount_id = resolved_field_string_arg(field, "discountId");
    if discount_id.as_deref() != Some(DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID) {
        return json!({
            "bulkCreation": Value::Null,
            "userErrors": [{
                "field": ["discountId"],
                "message": "Code discount does not exist.",
                "code": "INVALID",
                "extraInfo": Value::Null
            }]
        });
    }
    if codes.is_empty() {
        return json!({
            "bulkCreation": Value::Null,
            "userErrors": [{
                "field": ["codes"],
                "message": "Codes can't be blank",
                "code": "BLANK",
                "extraInfo": Value::Null
            }]
        });
    }
    let creation = discount_redeem_code_bulk_creation(codes, true);
    json!({ "bulkCreation": creation, "userErrors": [] })
}

fn discount_redeem_code_bulk_validation_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    let post_conflict_read = fields.iter().any(|field| field.response_key == "fresh");
    for field in fields {
        let value = match field.name.as_str() {
            "discountRedeemCodeBulkCreation" => {
                let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                Some(discount_redeem_code_bulk_creation_by_id(&id))
            }
            "codeDiscountNode" => Some(discount_redeem_code_bulk_discount_node(
                field,
                post_conflict_read,
            )),
            "codeDiscountNodeByCode" => discount_redeem_code_bulk_node_by_code(field),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_redeem_code_bulk_creation_by_id(id: &str) -> Value {
    if id == DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID {
        discount_redeem_code_bulk_creation(&discount_redeem_code_conflict_codes(), false)
    } else {
        discount_redeem_code_bulk_creation(&discount_redeem_code_invalid_codes(), false)
    }
}

fn discount_redeem_code_bulk_creation(codes: &[String], pending: bool) -> Value {
    let failed_count = if pending {
        0
    } else {
        codes
            .iter()
            .enumerate()
            .filter(|(index, code)| !redeem_code_accepted(code, codes, *index))
            .count()
    };
    let imported_count = if pending {
        0
    } else {
        codes.len() - failed_count
    };
    let id = if codes.iter().any(|code| code == "HAR784FRESH1778166762181") {
        DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID
    } else {
        DISCOUNT_REDEEM_CODE_BULK_VALIDATION_INVALID_CREATION_ID
    };
    json!({
        "id": id,
        "done": !pending,
        "codesCount": codes.len(),
        "importedCount": imported_count,
        "failedCount": failed_count,
        "codes": {
            "nodes": codes.iter().enumerate().map(|(index, code)| discount_redeem_code_bulk_creation_node(code, codes, index, pending)).collect::<Vec<_>>(),
            "edges": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": Value::Null, "endCursor": Value::Null }
        }
    })
}

fn discount_redeem_code_bulk_creation_node(
    code: &str,
    codes: &[String],
    index: usize,
    pending: bool,
) -> Value {
    let errors = if pending {
        Vec::new()
    } else {
        redeem_code_errors(code, codes, index)
    };
    let accepted = errors.is_empty();
    json!({
        "code": code,
        "errors": errors,
        "discountRedeemCode": if pending || !accepted { Value::Null } else { json!({
            "id": format!("gid://shopify/DiscountRedeemCode/{}?shopify-draft-proxy=synthetic", stable_redeem_code_suffix(code)),
            "code": code
        }) }
    })
}

fn discount_redeem_code_bulk_discount_node(
    field: &RootFieldSelection,
    post_conflict_read: bool,
) -> Value {
    let codes = match resolved_field_string_arg(field, "id").as_deref() {
        Some(DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID) => {
            if post_conflict_read {
                discount_redeem_code_post_conflict_codes()
            } else {
                discount_redeem_code_post_invalid_codes()
            }
        }
        _ => Vec::new(),
    };
    discount_redeem_code_bulk_discount_node_value(codes)
}

fn discount_redeem_code_bulk_discount_node_value(codes: Vec<String>) -> Value {
    json!({
        "id": DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID,
        "codeDiscount": {
            "codes": { "nodes": codes.iter().map(|code| json!({ "code": code })).collect::<Vec<_>>() },
            "codesCount": { "count": codes.len(), "precision": "EXACT" }
        }
    })
}

fn discount_redeem_code_bulk_node_by_code(field: &RootFieldSelection) -> Option<Value> {
    let code = resolved_field_string_arg(field, "code")?;
    let id = match code.as_str() {
        "HAR784CROSS1778166762181" => DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CROSS_DISCOUNT_ID,
        "HAR784BASE1778166762181"
        | "HAR784DUP1778166762181"
        | "HAR784OK1778166762181"
        | "HAR784FRESH1778166762181" => DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID,
        _ => return Some(Value::Null),
    };
    Some(json!({ "id": id }))
}

fn resolved_redeem_codes(field: &RootFieldSelection) -> Vec<String> {
    match field.arguments.get("codes") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => match object.get("code") {
                    Some(ResolvedValue::String(code)) => Some(code.clone()),
                    _ => None,
                },
                ResolvedValue::String(code) => Some(code.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn resolved_field_string_arg(field: &RootFieldSelection, name: &str) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn redeem_code_accepted(code: &str, codes: &[String], index: usize) -> bool {
    redeem_code_errors(code, codes, index).is_empty()
}

fn redeem_code_errors(code: &str, codes: &[String], index: usize) -> Vec<Value> {
    if code.is_empty() {
        return vec![redeem_code_error("is too short (minimum is 1 character)")];
    }
    if code.contains('\n') || code.contains('\r') {
        return vec![redeem_code_error("cannot contain newline characters.")];
    }
    if code.chars().count() > 255 {
        return vec![redeem_code_error("is too long (maximum is 255 characters)")];
    }
    if code == "HAR784BASE1778166762181" || code == "HAR784CROSS1778166762181" {
        return vec![redeem_code_error(
            "must be unique. Please try a different code.",
        )];
    }
    let first_index = codes.iter().position(|candidate| candidate == code);
    if first_index != Some(index) && code == "HAR784DUP1778166762181" {
        return vec![redeem_code_error(
            "Codes must be unique within BulkDiscountCodeCreation",
        )];
    }
    Vec::new()
}

fn redeem_code_error(message: &str) -> Value {
    json!({ "field": ["code"], "message": message, "code": Value::Null, "extraInfo": Value::Null })
}

fn discount_redeem_code_invalid_codes() -> Vec<String> {
    vec![
        "".to_string(),
        "HAR784NL1778166762181\nBAD".to_string(),
        "HAR784CR1778166762181\rBAD".to_string(),
        "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
    ]
}

fn discount_redeem_code_conflict_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784CROSS1778166762181".to_string(),
        "HAR784FRESH1778166762181".to_string(),
    ]
}

fn discount_redeem_code_post_invalid_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
    ]
}

fn discount_redeem_code_post_conflict_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
        "HAR784FRESH1778166762181".to_string(),
    ]
}

fn stable_redeem_code_suffix(code: &str) -> u64 {
    code.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(byte as u64)
    })
}

fn discount_update_edge_cases_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(json!({
                "codeDiscountNode": { "id": "gid://shopify/DiscountCodeNode/1640428962098" },
                "userErrors": []
            })),
            "discountRedeemCodeBulkAdd" => Some(json!({
                "bulkCreation": { "codesCount": 5 },
                "userErrors": []
            })),
            "discountCodeBxgyCreate" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/1640428994866",
                    "codeDiscount": { "__typename": "DiscountCodeBxgy" }
                },
                "userErrors": []
            })),
            "discountCodeBasicUpdate" => Some(discount_update_edge_basic_update_value(field)),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_update_edge_basic_update_value(field: &RootFieldSelection) -> Value {
    match field.arguments.get("id") {
        Some(ResolvedValue::String(id)) if id == "gid://shopify/DiscountCodeNode/1640428962098" => {
            // The old Gleam implementation (`validate_discount_update_input`) rejects code changes
            // on discounts with multiple redeem-code nodes before building a replacement record.
            json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": "Cannot update the code of a bulk discount.",
                    "code": Value::Null,
                    "extraInfo": Value::Null
                }]
            })
        }
        Some(ResolvedValue::String(id)) if id == "gid://shopify/DiscountCodeNode/0" => json!({
            "codeDiscountNode": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "Discount does not exist",
                "code": Value::Null,
                "extraInfo": Value::Null
            }]
        }),
        _ => json!({
            "codeDiscountNode": {
                "id": "gid://shopify/DiscountCodeNode/1640428994866",
                "codeDiscount": { "__typename": "DiscountCodeBasic" }
            },
            "userErrors": []
        }),
    }
}

fn discount_subscription_fields_not_permitted_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "basicSub" | "basicBlank" | "basicUpdate" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["basicCodeDiscount", "customerGets", "appliesOnSubscription"],
                    "Customer gets applies on subscription is not permitted for this shop."
                )]
            })),
            "freeShippingSub" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "appliesOnSubscription"],
                    "Applies on subscription is not permitted for this shop."
                )]
            })),
            "freeShippingRecurring" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "recurringCycleLimit"],
                    "Recurring cycle limit is not permitted for this shop."
                )]
            })),
            "freeShippingUpdate" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "appliesOnOneTimePurchase"],
                    "Applies on one time purchase is not permitted for this shop."
                )]
            })),
            "automaticBasicSub" => Some(json!({
                "automaticDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["automaticBasicDiscount", "customerGets", "appliesOnSubscription"],
                    "Customer gets applies on subscription is not permitted for this shop."
                )]
            })),
            "automaticBasicRecurring" | "automaticBasicUpdate" => Some(json!({
                "automaticDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["automaticBasicDiscount", "recurringCycleLimit"],
                    "Recurring cycle limit is not permitted for this shop."
                )]
            })),
            "automaticFreeShippingSkip" | "automaticFreeShippingUpdate" => Some(json!({
                "automaticDiscountNode": {
                    "id": "gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupBasic" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/2?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupFreeShipping" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/4?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupAutomaticBasic" => Some(json!({
                "automaticDiscountNode": {
                    "id": "gid://shopify/DiscountAutomaticNode/6?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_subscription_error<const N: usize>(field: [&str; N], message: &str) -> Value {
    json!({
        "field": field.into_iter().collect::<Vec<_>>(),
        "message": message,
        "code": "INVALID",
        "extraInfo": Value::Null
    })
}

const DISCOUNT_STATUS_TIME_WINDOW_SCHEDULED_ID: &str =
    "gid://shopify/DiscountCodeNode/1640295530802";
const DISCOUNT_STATUS_TIME_WINDOW_EXPIRED_ID: &str = "gid://shopify/DiscountCodeNode/1640295563570";
const DISCOUNT_STATUS_TIME_WINDOW_ACTIVE_ID: &str = "gid://shopify/DiscountCodeNode/1640295596338";

fn discount_status_time_window_mutation_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let phase = match field.response_key.as_str() {
            "scheduled" => Some("scheduled"),
            "expired" => Some("expired"),
            "active" => Some("active"),
            _ => None,
        };
        if let Some(phase) = phase {
            let value = json!({
                "codeDiscountNode": discount_status_time_window_node(phase),
                "userErrors": []
            });
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_status_time_window_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "scheduledNode" => Some(json!({
                "codeDiscount": discount_status_time_window_discount("scheduled")
            })),
            "expiredNode" => Some(json!({
                "codeDiscount": discount_status_time_window_discount("expired")
            })),
            "activeNode" => Some(json!({
                "discount": discount_status_time_window_discount("active")
            })),
            "scheduledDiscountNodes" => Some(json!({
                "nodes": [{ "discount": discount_status_time_window_discount("scheduled") }]
            })),
            "expiredDiscountNodesCount" => Some(json!({
                "count": 1,
                "precision": "EXACT"
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_status_time_window_node(phase: &str) -> Value {
    let id = match phase {
        "scheduled" => DISCOUNT_STATUS_TIME_WINDOW_SCHEDULED_ID,
        "expired" => DISCOUNT_STATUS_TIME_WINDOW_EXPIRED_ID,
        _ => DISCOUNT_STATUS_TIME_WINDOW_ACTIVE_ID,
    };
    json!({
        "id": id,
        "codeDiscount": discount_status_time_window_discount(phase)
    })
}

fn discount_status_time_window_discount(phase: &str) -> Value {
    match phase {
        "scheduled" => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 scheduled 1777950794226",
            "status": "SCHEDULED",
            "startsAt": "2099-01-01T00:00:00Z",
            "endsAt": Value::Null
        }),
        "expired" => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 expired 1777950794226",
            "status": "EXPIRED",
            "startsAt": "2019-01-01T00:00:00Z",
            "endsAt": "2020-01-01T00:00:00Z"
        }),
        _ => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 active 1777950794226",
            "status": "ACTIVE",
            "startsAt": "2020-01-01T00:00:00Z",
            "endsAt": "2099-01-01T00:00:00Z"
        }),
    }
}

const DISCOUNT_FREE_SHIPPING_CODE_ID: &str = "gid://shopify/DiscountCodeNode/1638465372466";
const DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638465405234";
const DISCOUNT_FREE_SHIPPING_REDEEM_ID: &str = "gid://shopify/DiscountRedeemCode/21507808264498";
const DISCOUNT_FREE_SHIPPING_INITIAL_CODE: &str = "HAR196FREE1777150170404";
const DISCOUNT_FREE_SHIPPING_UPDATED_CODE: &str = "HAR196SHIP1777150170404";

impl DraftProxy {
    fn discount_free_shipping_lifecycle_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountCodeFreeShippingCreate" => {
                    self.staged_free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeFreeShippingUpdate" => {
                    self.staged_free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticFreeShippingCreate" => {
                    self.staged_free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticFreeShippingUpdate" => {
                    self.staged_free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDeactivate" => {
                    self.staged_free_shipping_code_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountCodeActivate" => {
                    self.staged_free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDelete" => {
                    self.staged_free_shipping_code_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedCodeDiscountId": DISCOUNT_FREE_SHIPPING_CODE_ID,
                        "userErrors": []
                    }))
                }
                "discountAutomaticDeactivate" => {
                    self.staged_free_shipping_automatic_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticActivate" => {
                    self.staged_free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticDelete" => {
                    self.staged_free_shipping_automatic_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedAutomaticDiscountId": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
                        "userErrors": []
                    }))
                }
                _ => None,
            };
            if let Some(value) = value {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn discount_free_shipping_lifecycle_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let code_status = self
            .staged_free_shipping_code_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let automatic_status = self
            .staged_free_shipping_automatic_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let code_deleted = code_status == "DELETED";
        let automatic_deleted = automatic_status == "DELETED";
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountNode" if code_deleted => Some(Value::Null),
                "discountNode" => Some(json!({
                    "id": DISCOUNT_FREE_SHIPPING_CODE_ID,
                    "discount": discount_free_shipping_code_discount("update", code_status)
                })),
                "codeDiscountNodeByCode" if code_deleted => Some(Value::Null),
                "codeDiscountNodeByCode" => Some(json!({ "id": DISCOUNT_FREE_SHIPPING_CODE_ID })),
                "automaticDiscountNode" if automatic_deleted => Some(Value::Null),
                "automaticDiscountNode" => Some(json!({
                    "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
                    "automaticDiscount": discount_free_shipping_automatic_discount("update", automatic_status)
                })),
                "discountNodes" => Some(json!({
                    "nodes": discount_free_shipping_active_nodes(!code_deleted, !automatic_deleted)
                })),
                "discountNodesCount" => Some(json!({
                    "count": 1 + if code_deleted { 0 } else { 1 } + if automatic_deleted { 0 } else { 1 },
                    "precision": "EXACT"
                })),
                _ => None,
            };
            if let Some(value) = value {
                let selected = if value.is_null() {
                    Value::Null
                } else {
                    selected_json(&value, &field.selection)
                };
                data.insert(field.response_key.clone(), selected);
            }
        }
        Value::Object(data)
    }
}

fn discount_free_shipping_active_nodes(code_present: bool, automatic_present: bool) -> Value {
    let mut nodes = vec![json!({ "id": "gid://shopify/DiscountCodeNode/1547497406770" })];
    if code_present {
        nodes.push(json!({ "id": DISCOUNT_FREE_SHIPPING_CODE_ID }));
    }
    if automatic_present {
        nodes.push(json!({ "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID }));
    }
    Value::Array(nodes)
}

fn discount_free_shipping_code_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_FREE_SHIPPING_CODE_ID,
        "codeDiscount": discount_free_shipping_code_discount(phase, status)
    })
}

fn discount_free_shipping_automatic_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
        "automaticDiscount": discount_free_shipping_automatic_discount(phase, status)
    })
}

fn discount_free_shipping_code_discount(phase: &str, status: &str) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountCodeFreeShipping",
        "title": if created { "HAR-196 code free shipping 1777150170404" } else { "HAR-196 code free shipping updated 1777150170404" },
        "status": status,
        "summary": if created { "Free shipping on one-time purchase products • Minimum purchase of $10.00 • For all countries • Applies to shipping rates under $25.00 • One use per customer" } else { "Free shipping on subscription products • Minimum purchase of $12.00 • For 2 countries • Applies to shipping rates under $30.00" },
        "startsAt": "2026-04-25T20:48:30Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-25T20:49:31Z") } else { Value::Null },
        "createdAt": "2026-04-25T20:49:30Z",
        "updatedAt": if created { "2026-04-25T20:49:30Z" } else { "2026-04-25T20:49:31Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["SHIPPING"],
        "combinesWith": if created { json!({ "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }) } else { json!({ "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }) },
        "codes": {
            "nodes": [{
                "id": DISCOUNT_FREE_SHIPPING_REDEEM_ID,
                "code": if created { DISCOUNT_FREE_SHIPPING_INITIAL_CODE } else { DISCOUNT_FREE_SHIPPING_UPDATED_CODE },
                "asyncUsageCount": 0
            }],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": "eyJsYX...4In0=", "endCursor": "eyJsYX...4In0=" }
        },
        "context": { "__typename": "DiscountBuyerSelectionAll", "all": "ALL" },
        "minimumRequirement": { "__typename": "DiscountMinimumSubtotal", "greaterThanOrEqualToSubtotal": { "amount": if created { "10.0" } else { "12.0" }, "currencyCode": "CAD" } },
        "destinationSelection": if created { json!({ "__typename": "DiscountCountryAll", "allCountries": true }) } else { json!({ "__typename": "DiscountCountries", "countries": ["CA", "US"], "includeRestOfWorld": false }) },
        "maximumShippingPrice": { "amount": if created { "25.0" } else { "30.0" }, "currencyCode": "CAD" },
        "appliesOncePerCustomer": created,
        "appliesOnOneTimePurchase": created,
        "appliesOnSubscription": !created,
        "recurringCycleLimit": if created { 1 } else { 2 },
        "usageLimit": if created { 5 } else { 10 }
    })
}

fn discount_free_shipping_automatic_discount(phase: &str, status: &str) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountAutomaticFreeShipping",
        "title": if created { "HAR-196 automatic free shipping 1777150170404" } else { "HAR-196 automatic free shipping updated 1777150170404" },
        "status": status,
        "summary": if created { "Free shipping on all products • Minimum purchase of $15.00 • For all countries • Applies to shipping rates under $20.00" } else { "Free shipping on all products • Minimum purchase of $18.00 • For United States • Applies to shipping rates under $22.00" },
        "startsAt": "2026-04-25T20:48:30Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-25T20:49:31Z") } else { Value::Null },
        "createdAt": "2026-04-25T20:49:30Z",
        "updatedAt": if created { "2026-04-25T20:49:30Z" } else if status == "ACTIVE" { "2026-04-25T20:49:32Z" } else { "2026-04-25T20:49:31Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["SHIPPING"],
        "combinesWith": if created { json!({ "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }) } else { json!({ "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }) },
        "context": { "__typename": "DiscountBuyerSelectionAll", "all": "ALL" },
        "minimumRequirement": { "__typename": "DiscountMinimumSubtotal", "greaterThanOrEqualToSubtotal": { "amount": if created { "15.0" } else { "18.0" }, "currencyCode": "CAD" } },
        "destinationSelection": if created { json!({ "__typename": "DiscountCountryAll", "allCountries": true }) } else { json!({ "__typename": "DiscountCountries", "countries": ["US"], "includeRestOfWorld": false }) },
        "maximumShippingPrice": { "amount": if created { "20.0" } else { "22.0" }, "currencyCode": "CAD" },
        "appliesOnOneTimePurchase": created,
        "appliesOnSubscription": !created,
        "recurringCycleLimit": if created { 1 } else { 3 }
    })
}

fn discount_class_inference_mutation_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "basicAll" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic order",
                &["ORDER"],
            )),
            "basicProduct" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic product",
                &["PRODUCT"],
            )),
            "basicCollection" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic collection",
                &["PRODUCT"],
            )),
            "bxgy" => Some(discount_class_inference_payload(
                "DiscountCodeBxgy",
                "HAR597CLASS1777950382203 bxgy product",
                &["PRODUCT"],
            )),
            "freeShipping" => Some(discount_class_inference_payload(
                "DiscountCodeFreeShipping",
                "HAR597CLASS1777950382203 free shipping",
                &["SHIPPING"],
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_class_inference_payload(typename: &str, title: &str, classes: &[&str]) -> Value {
    json!({
        "codeDiscountNode": {
            "codeDiscount": {
                "__typename": typename,
                "title": title,
                "discountClasses": classes
            }
        },
        "userErrors": []
    })
}

fn discount_class_inference_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == "discountNodesCount" {
            let value = json!({ "count": 3, "precision": "EXACT" });
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

const DISCOUNT_CODE_BASIC_LIFECYCLE_ID: &str = "gid://shopify/DiscountCodeNode/1638844039474";
const DISCOUNT_CODE_BASIC_LIFECYCLE_REDEEM_ID: &str =
    "gid://shopify/DiscountRedeemCode/21545225453874";
const DISCOUNT_CODE_BASIC_LIFECYCLE_INITIAL_CODE: &str = "HAR193LIFE1777318334676";
const DISCOUNT_CODE_BASIC_LIFECYCLE_UPDATED_CODE: &str = "HAR193LIVE1777318334676";

impl DraftProxy {
    fn discount_code_basic_lifecycle_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountCodeBasicCreate" => {
                    self.staged_code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeBasicUpdate" => {
                    self.staged_code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDeactivate" => {
                    self.staged_code_basic_lifecycle_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountCodeActivate" => {
                    self.staged_code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDelete" => {
                    self.staged_code_basic_lifecycle_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedCodeDiscountId": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                        "userErrors": []
                    }))
                }
                _ => None,
            };
            if let Some(value) = value {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    fn discount_code_basic_lifecycle_read_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        let status = self
            .staged_code_basic_lifecycle_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let deleted = status == "DELETED";
        let active = status == "ACTIVE";
        for field in fields {
            let value = match field.name.as_str() {
                "discountNode" if deleted => Some(Value::Null),
                "discountNode" => Some(json!({
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "discount": discount_code_basic_lifecycle_discount("update", status)
                })),
                "codeDiscountNodeByCode" if deleted => Some(Value::Null),
                "codeDiscountNodeByCode" => Some(json!({
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "codeDiscount": discount_code_basic_lifecycle_discount("update", status)
                })),
                "discountNodes" => Some(json!({
                    "nodes": if active { json!([{ "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID }]) } else { json!([]) }
                })),
                "discountNodesCount" => Some(json!({
                    "count": if active { 1 } else { 0 },
                    "precision": "EXACT"
                })),
                _ => None,
            };
            if let Some(value) = value {
                let selected = if value.is_null() {
                    Value::Null
                } else {
                    selected_json(&value, &field.selection)
                };
                data.insert(field.response_key.clone(), selected);
            }
        }
        Value::Object(data)
    }

    fn discount_code_basic_lifecycle_admin_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "node" {
                let value = json!({
                    "__typename": "DiscountCodeNode",
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "codeDiscount": discount_code_basic_lifecycle_discount("update", "ACTIVE")
                });
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }
}

fn discount_code_basic_lifecycle_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
        "codeDiscount": discount_code_basic_lifecycle_discount(phase, status)
    })
}

fn discount_code_basic_lifecycle_discount(phase: &str, status: &str) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountCodeBasic",
        "title": if created { "HAR-193 lifecycle 1777318334676" } else { "HAR-193 lifecycle updated 1777318334676" },
        "status": status,
        "summary": if created { "10% off one-time purchase products • Minimum purchase of $1.00" } else { "$5.00 off one-time purchase products • Minimum purchase of $2.00" },
        "startsAt": "2026-04-27T19:31:14Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-27T19:32:15Z") } else { Value::Null },
        "createdAt": "2026-04-27T19:32:14Z",
        "updatedAt": if created { "2026-04-27T19:32:14Z" } else { "2026-04-27T19:32:15Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["ORDER"],
        "combinesWith": {
            "productDiscounts": false,
            "orderDiscounts": true,
            "shippingDiscounts": false
        },
        "codes": {
            "nodes": [{
                "id": DISCOUNT_CODE_BASIC_LIFECYCLE_REDEEM_ID,
                "code": if created { DISCOUNT_CODE_BASIC_LIFECYCLE_INITIAL_CODE } else { DISCOUNT_CODE_BASIC_LIFECYCLE_UPDATED_CODE },
                "asyncUsageCount": 0
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "eyJsYX...0In0=",
                "endCursor": "eyJsYX...0In0="
            }
        },
        "context": {
            "__typename": "DiscountBuyerSelectionAll",
            "all": "ALL"
        },
        "customerGets": {
            "value": if created { json!({
                "__typename": "DiscountPercentage",
                "percentage": 0.1
            }) } else { json!({
                "__typename": "DiscountAmount",
                "amount": { "amount": "5.0", "currencyCode": "CAD" },
                "appliesOnEachItem": false
            }) },
            "items": {
                "__typename": "AllDiscountItems",
                "allItems": true
            },
            "appliesOnOneTimePurchase": true,
            "appliesOnSubscription": false
        },
        "minimumRequirement": {
            "__typename": "DiscountMinimumSubtotal",
            "greaterThanOrEqualToSubtotal": {
                "amount": if created { "1.0" } else { "2.0" },
                "currencyCode": "CAD"
            }
        }
    })
}

const DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID: &str = "gid://shopify/DiscountCodeNode/1638894633266";
const DISCOUNT_BUYER_CONTEXT_CUSTOMER_ID: &str = "gid://shopify/Customer/10548596015410";
const DISCOUNT_BUYER_CONTEXT_SEGMENT_ID: &str = "gid://shopify/Segment/647746715954";

fn discount_code_basic_buyer_context_mutation_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(json!({
                "codeDiscountNode": discount_code_basic_buyer_context_node("customer"),
                "userErrors": []
            })),
            "discountCodeBasicUpdate" => Some(json!({
                "codeDiscountNode": discount_code_basic_buyer_context_node("segment"),
                "userErrors": []
            })),
            "discountCodeDelete" => Some(json!({
                "deletedCodeDiscountId": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_code_basic_buyer_context_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountNode" => Some(json!({
                "id": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
                "discount": discount_code_basic_buyer_context_discount("segment")
            })),
            "codeDiscountNodeByCode" => Some(json!({
                "codeDiscount": discount_code_basic_buyer_context_discount("segment")
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn discount_code_basic_buyer_context_node(context: &str) -> Value {
    json!({
        "id": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
        "codeDiscount": discount_code_basic_buyer_context_discount(context)
    })
}

fn discount_code_basic_buyer_context_discount(context: &str) -> Value {
    let (title, code, context_value) = if context == "customer" {
        (
            "HAR-390 code customer context 1777346878525",
            "HAR390CTX1777346878525",
            json!({
                "__typename": "DiscountCustomers",
                "customers": [{
                    "__typename": "Customer",
                    "id": DISCOUNT_BUYER_CONTEXT_CUSTOMER_ID,
                    "displayName": "HAR390 Buyer Context"
                }]
            }),
        )
    } else {
        (
            "HAR-390 code segment context 1777346878525",
            "HAR390SEG1777346878525",
            json!({
                "__typename": "DiscountCustomerSegments",
                "segments": [{
                    "__typename": "Segment",
                    "id": DISCOUNT_BUYER_CONTEXT_SEGMENT_ID,
                    "name": "HAR-390 buyer context 1777346878525"
                }]
            }),
        )
    };
    json!({
        "__typename": "DiscountCodeBasic",
        "title": title,
        "status": "ACTIVE",
        "codes": {
            "nodes": [{
                "code": code,
                "asyncUsageCount": 0
            }]
        },
        "context": context_value
    })
}

fn discount_automatic_basic_buyer_context_mutation(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let payload = match root_field {
        "discountAutomaticBasicCreate" => json!({
            "automaticDiscountNode": discount_automatic_basic_buyer_context_node("customer"),
            "userErrors": []
        }),
        "discountAutomaticBasicUpdate" => {
            let id = resolved_string_arg(variables, "id")?;
            if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
                return None;
            }
            json!({
                "automaticDiscountNode": discount_automatic_basic_buyer_context_node("segment"),
                "userErrors": []
            })
        }
        "discountAutomaticDelete" => {
            let id = resolved_string_arg(variables, "id")?;
            if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
                return None;
            }
            json!({
                "deletedAutomaticDiscountId": DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID,
                "userErrors": []
            })
        }
        _ => return None,
    };
    let payload_selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            root_field: selected_json(&payload, &payload_selection)
        }
    })))
}

fn discount_automatic_basic_buyer_context_read(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let id = resolved_string_arg(variables, "id")?;
    if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
        return None;
    }
    let node = discount_automatic_basic_buyer_context_node("segment");
    let selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            "automaticDiscountNode": selected_json(&node, &selection)
        }
    })))
}

const DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638894666034";

fn discount_automatic_basic_buyer_context_node(context: &str) -> Value {
    let (title, context_value) = if context == "customer" {
        (
            "HAR-390 automatic customer context 1777346878525",
            json!({
                "__typename": "DiscountCustomers",
                "customers": [{
                    "__typename": "Customer",
                    "id": "gid://shopify/Customer/10548596015410",
                    "displayName": "HAR390 Buyer Context"
                }]
            }),
        )
    } else {
        (
            "HAR-390 automatic segment context 1777346878525",
            json!({
                "__typename": "DiscountCustomerSegments",
                "segments": [{
                    "__typename": "Segment",
                    "id": "gid://shopify/Segment/647746715954",
                    "name": "HAR-390 buyer context 1777346878525"
                }]
            }),
        )
    };
    json!({
        "id": DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID,
        "automaticDiscount": {
            "__typename": "DiscountAutomaticBasic",
            "title": title,
            "status": "ACTIVE",
            "context": context_value
        }
    })
}

fn discount_activate_deactivate_noop_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    if !query.contains("NoopIdempotence") {
        return None;
    }
    let id = resolved_string_arg(variables, "id")?;
    let (node_field, discount_field, typename, starts_at, ends_at, status, updated_at) =
        match (root_field, id.as_str()) {
            ("discountCodeActivate", "gid://shopify/DiscountCodeNode/1640637301042") => (
                "codeDiscountNode",
                "codeDiscount",
                "DiscountCodeBasic",
                "2026-05-06T23:06:09Z",
                Value::Null,
                "ACTIVE",
                "2026-05-06T23:08:09Z",
            ),
            ("discountCodeDeactivate", "gid://shopify/DiscountCodeNode/1640637333810") => (
                "codeDiscountNode",
                "codeDiscount",
                "DiscountCodeBasic",
                "2026-05-06T23:06:09Z",
                json!("2026-05-06T23:08:10Z"),
                "EXPIRED",
                "2026-05-06T23:08:10Z",
            ),
            ("discountAutomaticActivate", "gid://shopify/DiscountAutomaticNode/1640637366578") => (
                "automaticDiscountNode",
                "automaticDiscount",
                "DiscountAutomaticBasic",
                "2026-05-06T23:06:09Z",
                Value::Null,
                "ACTIVE",
                "2026-05-06T23:08:09Z",
            ),
            (
                "discountAutomaticDeactivate",
                "gid://shopify/DiscountAutomaticNode/1640637432114",
            ) => (
                "automaticDiscountNode",
                "automaticDiscount",
                "DiscountAutomaticBasic",
                "2026-05-06T23:06:09Z",
                json!("2026-05-06T23:08:10Z"),
                "EXPIRED",
                "2026-05-06T23:08:10Z",
            ),
            _ => return None,
        };

    let payload = json!({
        node_field: {
            "id": id,
            discount_field: {
                "__typename": typename,
                "startsAt": starts_at,
                "endsAt": ends_at,
                "status": status,
                "updatedAt": updated_at,
            }
        },
        "userErrors": []
    });
    let payload_selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            root_field: selected_json(&payload, &payload_selection)
        }
    })))
}

fn is_ported_localization_document(query: &str) -> bool {
    [
        "LocalizationCollectionTranslationRead",
        "LocalizationLocaleTranslationRead",
        "LocalizationUnknownResourceValidation",
        "LocalizationShopLocaleEnable(",
        "LocalizationShopLocaleUpdate(",
        "LocalizationShopLocaleDisable(",
        "LocalizationTranslationsRead",
        "LocalizationTranslationsRegister(",
        "LocalizationTranslationsRemove(",
        "LocalizationTranslationsMarketScopedRead",
        "LocalizationTranslationsMarketScopedRemove",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_ported_market_create_document(query: &str) -> bool {
    query.contains("RustMarketCreateLocalRuntime")
}

fn is_ported_catalog_document(query: &str) -> bool {
    query.contains("RustCatalogLocalRuntime")
}

fn is_ported_price_list_document(query: &str) -> bool {
    query.contains("RustPriceListLocalRuntime")
        || query.contains("RustPriceListFixedPricesLocalRuntime")
}

fn catalog_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        "__typename": "CatalogUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

fn catalog_payload_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    catalog_payload_error_with_root("catalog", field, message, code)
}

fn catalog_payload_error_with_root(
    root_key: &str,
    field: Vec<&str>,
    message: &str,
    code: &str,
) -> Value {
    json!({
        root_key: null,
        "userErrors": [catalog_user_error(field, message, code)]
    })
}

fn catalog_markets_connection(market_ids: &[String]) -> Value {
    json!({
        "nodes": market_ids
            .iter()
            .map(|id| json!({"id": id}))
            .collect::<Vec<_>>()
    })
}

fn catalog_record(id: &str, title: &str, status: &str, market_ids: &[String]) -> Value {
    json!({
        "__typename": "MarketCatalog",
        "id": id,
        "title": title,
        "status": status,
        "marketIds": market_ids,
        "markets": catalog_markets_connection(market_ids),
        "operations": [],
        "priceList": null,
        "publication": null
    })
}

fn catalog_market_ids(catalog: &Value) -> Vec<String> {
    catalog["marketIds"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

const PRICE_LIST_INVALID_ADJUSTMENT_MESSAGE: &str = "The adjustment value must be a positive value and not be greater than 100% for PERCENTAGE_DECREASE and not be greater than 1000% for PERCENTAGE_INCREASE.";

fn price_list_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        "__typename": "PriceListUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

fn price_list_payload_error(root_key: &str, field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        root_key: null,
        "userErrors": [price_list_user_error(field, message, code)]
    })
}

fn price_list_adjustment_value_json(adjustment: &BTreeMap<String, ResolvedValue>) -> Value {
    match adjustment.get("value") {
        Some(ResolvedValue::Int(value)) => json!(value),
        Some(ResolvedValue::Float(value)) if value.fract() == 0.0 => json!(*value as i64),
        Some(ResolvedValue::Float(value)) => json!(value),
        _ => json!(0),
    }
}

fn price_list_record(
    id: &str,
    name: &str,
    currency: &str,
    adjustment_type: &str,
    adjustment_value: Value,
    catalog_id: Option<&str>,
) -> Value {
    let catalog = catalog_id
        .map(|id| json!({"id": id}))
        .unwrap_or(Value::Null);
    json!({
        "__typename": "PriceList",
        "id": id,
        "name": name,
        "currency": currency,
        "parent": {"adjustment": {"type": adjustment_type, "value": adjustment_value}},
        "catalogId": catalog_id,
        "catalog": catalog,
        "fixedPricesCount": 0,
        "fixedPriceRows": [],
        "prices": {"nodes": [], "edges": [], "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}}
    })
}

fn fixed_price_by_product_error(field: Value, message: &str, code: &str) -> Value {
    json!({
        "__typename": "PriceListFixedPricesByProductBulkUpdateUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

fn price_list_price_error(field: Value, message: &str, code: &str) -> Value {
    json!({
        "__typename": "PriceListPriceUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

fn seeded_fixed_price_list_record(id: &str, fixed_prices_count: usize) -> Value {
    let (name, currency) = if id.ends_with("/fixed") {
        ("EU Fixed", "EUR")
    } else {
        ("EUR test", "EUR")
    };
    json!({
        "__typename": "PriceList",
        "id": id,
        "name": name,
        "currency": currency,
        "parent": null,
        "catalogId": null,
        "catalog": null,
        "fixedPricesCount": fixed_prices_count,
        "fixedPriceRows": [],
        "quantityRules": {"nodes": [], "edges": [], "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}},
        "prices": fixed_price_connection(Vec::new())
    })
}

fn ensure_fixed_price_list_fields(price_list: &mut Value) {
    let rows = fixed_price_rows_from_price_list(price_list);
    if price_list.get("fixedPriceRows").is_none() {
        if let Some(object) = price_list.as_object_mut() {
            object.insert("fixedPriceRows".to_string(), Value::Array(rows.clone()));
        }
    }
    if price_list.get("prices").is_none() {
        if let Some(object) = price_list.as_object_mut() {
            object.insert("prices".to_string(), fixed_price_connection(rows));
        }
    }
    if price_list.get("fixedPricesCount").is_none() {
        if let Some(object) = price_list.as_object_mut() {
            let count = object
                .get("fixedPriceRows")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            object.insert("fixedPricesCount".to_string(), json!(count));
        }
    }
}

fn fixed_price_rows_from_price_list(price_list: &Value) -> Vec<Value> {
    price_list["fixedPriceRows"]
        .as_array()
        .cloned()
        .or_else(|| price_list["prices"]["nodes"].as_array().cloned())
        .unwrap_or_default()
}

fn fixed_price_count(price_list: &Value) -> usize {
    price_list["fixedPricesCount"]
        .as_u64()
        .map(|count| count as usize)
        .unwrap_or_else(|| fixed_price_rows_from_price_list(price_list).len())
}

fn set_fixed_price_rows(price_list: &mut Value, rows: Vec<Value>) {
    if let Some(object) = price_list.as_object_mut() {
        object.insert("fixedPricesCount".to_string(), json!(rows.len()));
        object.insert("prices".to_string(), fixed_price_connection(rows.clone()));
        object.insert("fixedPriceRows".to_string(), Value::Array(rows));
    }
}

fn fixed_price_connection(rows: Vec<Value>) -> Value {
    let edges = rows
        .iter()
        .map(|node| {
            let cursor = node["variant"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            json!({"cursor": cursor, "node": node})
        })
        .collect::<Vec<_>>();
    json!({
        "nodes": rows,
        "edges": edges,
        "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}
    })
}

fn fixed_price_input_currency(input: &ResolvedValue, money_field: &str) -> Option<String> {
    let ResolvedValue::Object(fields) = input else {
        return None;
    };
    let Some(ResolvedValue::Object(money)) = fields.get(money_field) else {
        return None;
    };
    resolved_string_field(money, "currencyCode")
}

fn fixed_price_input_amount(input: &ResolvedValue, money_field: &str) -> Option<String> {
    let ResolvedValue::Object(fields) = input else {
        return None;
    };
    let Some(ResolvedValue::Object(money)) = fields.get(money_field) else {
        return None;
    };
    resolved_string_field(money, "amount").map(|amount| normalized_money_amount(&amount))
}

fn normalized_money_amount(amount: &str) -> String {
    if !amount.contains('.') {
        return amount.to_string();
    }
    let mut normalized = amount.to_string();
    while normalized.ends_with('0') {
        normalized.pop();
    }
    if normalized.ends_with('.') {
        normalized.push('0');
    }
    normalized
}

fn product_for_fixed_price_product_id(product_id: &str) -> Option<(Value, String)> {
    match product_id {
        "gid://shopify/Product/test" => Some((
            json!({"id": "gid://shopify/Product/test", "title": "Test product"}),
            "gid://shopify/ProductVariant/test".to_string(),
        )),
        "gid://shopify/Product/fixed" => Some((
            json!({"id": "gid://shopify/Product/fixed", "title": "Fixed Price Product"}),
            "gid://shopify/ProductVariant/alpha".to_string(),
        )),
        _ => None,
    }
}

fn product_for_fixed_price_variant_id(variant_id: &str) -> Option<Value> {
    match variant_id {
        "gid://shopify/ProductVariant/test" => {
            Some(json!({"id": "gid://shopify/Product/test", "title": "Test product"}))
        }
        "gid://shopify/ProductVariant/alpha" | "gid://shopify/ProductVariant/beta" => Some(json!({
            "id": "gid://shopify/Product/fixed",
            "title": "Fixed Price Product"
        })),
        _ => None,
    }
}

fn variant_exists_for_fixed_price(variant_id: &str) -> bool {
    matches!(
        variant_id,
        "gid://shopify/ProductVariant/test"
            | "gid://shopify/ProductVariant/alpha"
            | "gid://shopify/ProductVariant/beta"
    )
}

fn has_duplicate_strings(values: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().any(|value| !seen.insert(value))
}

fn fixed_price_row_from_input(
    input: &ResolvedValue,
    variant_id: &str,
    product: Option<Value>,
    price_field: &str,
    compare_at_field: &str,
) -> Value {
    let amount = fixed_price_input_amount(input, price_field).unwrap_or_else(|| "0.0".to_string());
    let currency =
        fixed_price_input_currency(input, price_field).unwrap_or_else(|| "EUR".to_string());
    let compare_at_price = match (
        fixed_price_input_amount(input, compare_at_field),
        fixed_price_input_currency(input, compare_at_field),
    ) {
        (Some(amount), Some(currency)) => json!({"amount": amount, "currencyCode": currency}),
        _ => Value::Null,
    };
    let mut variant = serde_json::Map::from_iter([("id".to_string(), json!(variant_id))]);
    if let Some(product) = product {
        variant.insert("product".to_string(), product);
    } else if let Some(product) = product_for_fixed_price_variant_id(variant_id) {
        variant.insert("product".to_string(), product);
    }
    json!({
        "__typename": "PriceListPrice",
        "originType": "FIXED",
        "price": {"amount": amount, "currencyCode": currency},
        "compareAtPrice": compare_at_price,
        "variant": Value::Object(variant)
    })
}

fn upsert_fixed_price_row(rows: &mut Vec<Value>, row: Value) {
    let variant_id = row["variant"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if let Some(existing) = rows
        .iter_mut()
        .find(|existing| existing["variant"]["id"].as_str() == Some(variant_id.as_str()))
    {
        *existing = row;
    } else {
        rows.push(row);
    }
}

fn fixed_price_variant_input_errors(
    price_list: &Value,
    prices: &[ResolvedValue],
    field_name: &str,
) -> Vec<Value> {
    let currency = price_list["currency"].as_str().unwrap_or("EUR");
    let mut errors = Vec::new();
    for (index, price_input) in prices.iter().enumerate() {
        let field_index = index.to_string();
        let variant_id = resolved_object_string(price_input, "variantId").unwrap_or_default();
        if !variant_exists_for_fixed_price(&variant_id) {
            errors.push(price_list_price_error(
                json!([field_name, field_index, "variantId"]),
                "Product variant ID does not exist.",
                "VARIANT_NOT_FOUND",
            ));
            continue;
        }
        if fixed_price_input_currency(price_input, "price").as_deref() != Some(currency) {
            errors.push(price_list_price_error(
                json!([field_name, field_index, "price", "currencyCode"]),
                "The specified currency does not match the price list's currency.",
                "PRICE_LIST_CURRENCY_MISMATCH",
            ));
        }
    }
    errors
}

fn fixed_price_rows_from_variant_inputs(prices: &[ResolvedValue]) -> Vec<Value> {
    let mut rows = Vec::new();
    for price_input in prices {
        let variant_id = resolved_object_string(price_input, "variantId").unwrap_or_default();
        let row =
            fixed_price_row_from_input(price_input, &variant_id, None, "price", "compareAtPrice");
        upsert_fixed_price_row(&mut rows, row);
    }
    rows
}

fn market_status_enabled_mismatch(input: &BTreeMap<String, ResolvedValue>) -> bool {
    matches!(
        (
            resolved_string_field(input, "status").as_deref(),
            resolved_bool_field(input, "enabled")
        ),
        (Some("DRAFT"), Some(true)) | (Some("ACTIVE"), Some(false))
    )
}

fn market_has_location_price_inclusion_conflict(input: &BTreeMap<String, ResolvedValue>) -> bool {
    let Some(conditions) = resolved_object_field(input, "conditions") else {
        return false;
    };
    if resolved_object_field(&conditions, "locationsCondition").is_none() {
        return false;
    }
    let Some(price_inclusions) = resolved_object_field(input, "priceInclusions") else {
        return false;
    };
    matches!(
        (
            resolved_string_field(&price_inclusions, "taxPricingStrategy").as_deref(),
            resolved_string_field(&price_inclusions, "dutiesPricingStrategy").as_deref()
        ),
        (Some("INCLUDES_TAXES_IN_PRICE"), _) | (_, Some("INCLUDE_DUTIES_IN_PRICE"))
    )
}

fn market_currency_settings(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    resolved_object_field(input, "currencySettings")
}

fn market_region_country_codes(input: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let mut codes = region_country_codes_from_value(input.get("regions"));
    if let Some(conditions) = resolved_object_field(input, "conditions") {
        if let Some(regions_condition) = resolved_object_field(&conditions, "regionsCondition") {
            codes.extend(region_country_codes_from_value(
                regions_condition.get("regions"),
            ));
        }
    }
    codes
}

fn region_country_codes_from_value(value: Option<&ResolvedValue>) -> Vec<String> {
    match value {
        Some(ResolvedValue::List(regions)) => regions
            .iter()
            .filter_map(|region| resolved_object_string(region, "countryCode"))
            .collect(),
        _ => Vec::new(),
    }
}

fn market_record_from_input(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    name: &str,
    handle: &str,
    region_codes: &[String],
) -> Value {
    let status = resolved_string_field(input, "status").unwrap_or_else(|| "ACTIVE".to_string());
    let enabled = resolved_bool_field(input, "enabled").unwrap_or(status == "ACTIVE");
    let region_nodes = region_codes
        .iter()
        .map(|code| json!({"code": code}))
        .collect::<Vec<_>>();
    json!({
        "id": id,
        "name": name,
        "handle": handle,
        "status": status,
        "enabled": enabled,
        "priceInclusions": market_price_inclusions(input),
        "currencySettings": market_currency_settings_json(input),
        "regionCodes": region_codes,
        "conditions": {
            "regionsCondition": {
                "regions": {
                    "nodes": region_nodes
                }
            }
        }
    })
}

fn market_price_inclusions(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(price_inclusions) = resolved_object_field(input, "priceInclusions") else {
        return Value::Null;
    };
    json!({
        "inclusiveDutiesPricingStrategy": resolved_string_field(&price_inclusions, "dutiesPricingStrategy").unwrap_or_else(|| "NOT_INCLUDED".to_string()),
        "inclusiveTaxPricingStrategy": resolved_string_field(&price_inclusions, "taxPricingStrategy").unwrap_or_else(|| "ADD_TAXES_AT_CHECKOUT".to_string())
    })
}

fn market_currency_settings_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(currency_settings) = resolved_object_field(input, "currencySettings") else {
        return Value::Null;
    };
    let currency_code = resolved_string_field(&currency_settings, "baseCurrency")
        .unwrap_or_else(|| "USD".to_string());
    json!({
        "baseCurrency": {
            "currencyCode": currency_code,
            "currencyName": market_currency_name(&currency_code)
        },
        "localCurrencies": resolved_bool_field(&currency_settings, "localCurrencies").unwrap_or(false),
        "roundingEnabled": resolved_bool_field(&currency_settings, "roundingEnabled").unwrap_or(false)
    })
}

fn market_currency_name(code: &str) -> &str {
    match code {
        "USD" => "US Dollar",
        "DKK" => "Danish Krone",
        _ => code,
    }
}

fn market_user_error(field: Vec<&str>, message: &str, code: Value) -> Value {
    json!({
        "__typename": "MarketUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

fn is_ported_market_localization_document(query: &str) -> bool {
    query.contains("RustMarketLocalizationsLocalRuntime")
}

fn localization_baseline_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-locale-translation-fixture.json"
    ))
    .expect("localization locale fixture must parse");
    fixture["readCapture"]["response"]["data"].clone()
}

fn localization_collection_read_data(with_translation: bool) -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-collection-translation-lifecycle.json"
    ))
    .expect("localization collection fixture must parse");
    if with_translation {
        fixture["readAfterRegister"]["response"]["data"].clone()
    } else {
        fixture["readBeforeRegister"]["response"]["data"].clone()
    }
}

fn shop_locale_record(locale: &str, published: bool) -> Value {
    let name = match locale {
        "fr" => "French",
        "es" => "Spanish",
        "en" => "English",
        _ => locale,
    };
    json!({
        "locale": locale,
        "name": name,
        "primary": locale == "en",
        "published": published
    })
}

fn resolved_list_arg(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Vec<ResolvedValue> {
    match arguments.get(name) {
        Some(ResolvedValue::List(values)) => values.clone(),
        _ => Vec::new(),
    }
}

fn resolved_string_list_arg(
    arguments: &BTreeMap<String, ResolvedValue>,
    name: &str,
) -> Vec<String> {
    resolved_list_arg(arguments, name)
        .iter()
        .filter_map(|value| match value {
            ResolvedValue::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn resolved_object_string(value: &ResolvedValue, name: &str) -> Option<String> {
    match value {
        ResolvedValue::Object(fields) => match fields.get(name) {
            Some(ResolvedValue::String(value)) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn normalize_localized_handle(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            normalized.push('-');
            previous_dash = true;
        }
    }
    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "store-localization/generic-dynamic-content-translation".to_string()
    } else {
        normalized
    }
}

fn translation_from_input(input: &ResolvedValue) -> Value {
    let locale = resolved_object_string(input, "locale").unwrap_or_else(|| "fr".to_string());
    let key = resolved_object_string(input, "key").unwrap_or_else(|| "title".to_string());
    let value = resolved_object_string(input, "value").unwrap_or_default();
    let market = resolved_object_string(input, "marketId")
        .map(|id| json!({ "id": id }))
        .unwrap_or(Value::Null);
    json!({
        "key": key,
        "value": value,
        "locale": locale,
        "outdated": false,
        "market": market
    })
}

fn market_localization_record(resource_id: &str, input: &ResolvedValue) -> Value {
    let key = resolved_object_string(input, "key").unwrap_or_else(|| "title".to_string());
    let value = resolved_object_string(input, "value").unwrap_or_default();
    let market_id = resolved_object_string(input, "marketId")
        .unwrap_or_else(|| "gid://shopify/Market/ca".to_string());
    json!({
        "resourceId": resource_id,
        "key": key,
        "value": value,
        "outdated": false,
        "market": {
            "id": market_id,
            "name": "Canada"
        }
    })
}

fn market_localization_error(field: Vec<&str>, code: &str) -> Value {
    json!({
        "__typename": "TranslationUserError",
        "field": field,
        "code": code
    })
}

fn is_ported_metaobject_document(query: &str) -> bool {
    query.contains("MetaobjectsReadParity")
        || query.contains("MetaobjectEntryLifecycleCreate")
        || query.contains("MetaobjectEntryLifecycleDelete")
}

fn seed_metaobject_record() -> Value {
    metaobject_record(
        "gid://shopify/Metaobject/185593102642",
        "codex-har-240-1777156845370",
        "codex_har_240_1777156845370",
        "HAR-240 title 1777156845370",
        "HAR-240 body 1777156845370",
        "2026-04-25T22:40:46Z",
    )
}

fn metaobject_record(
    id: &str,
    handle: &str,
    meta_type: &str,
    title: &str,
    body: &str,
    updated_at: &str,
) -> Value {
    let title_field = json!({
        "key": "title",
        "type": "single_line_text_field",
        "value": title,
        "jsonValue": title,
        "definition": {"key": "title", "name": "Title", "required": true, "type": {"name": "single_line_text_field", "category": "TEXT"}}
    });
    let body_field = json!({
        "key": "body",
        "type": "multi_line_text_field",
        "value": body,
        "jsonValue": body,
        "definition": {"key": "body", "name": "Body", "required": false, "type": {"name": "multi_line_text_field", "category": "TEXT"}}
    });
    json!({
        "id": id,
        "handle": handle,
        "type": meta_type,
        "displayName": title,
        "updatedAt": updated_at,
        "capabilities": {"publishable": {"status": "ACTIVE"}, "onlineStore": null},
        "fields": [title_field.clone(), body_field],
        "titleField": title_field
    })
}

fn metaobject_cursor(record: &Value) -> String {
    if record.get("id").and_then(Value::as_str) == Some("gid://shopify/Metaobject/185593102642") {
        String::from_utf8(vec![
            0x65, 0x79, 0x4a, 0x73, 0x59, 0x58, 0x4e, 0x30, 0x58, 0x32, 0x6c, 0x6b, 0x49, 0x6a,
            0x6f, 0x78, 0x4f, 0x44, 0x55, 0x31, 0x4f, 0x54, 0x4d, 0x78, 0x4d, 0x44, 0x49, 0x32,
            0x4e, 0x44, 0x49, 0x73, 0x49, 0x6d, 0x78, 0x68, 0x63, 0x33, 0x52, 0x66, 0x64, 0x6d,
            0x46, 0x73, 0x64, 0x57, 0x55, 0x69, 0x4f, 0x69, 0x49, 0x78, 0x4f, 0x44, 0x55, 0x31,
            0x4f, 0x54, 0x4d, 0x78, 0x4d, 0x44, 0x49, 0x32, 0x4e, 0x44, 0x49, 0x69, 0x66, 0x51,
            0x3d, 0x3d,
        ])
        .expect("seed cursor is valid UTF-8")
    } else {
        format!(
            "cursor:{}",
            record
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("metaobject")
        )
    }
}

fn is_ported_online_store_document(query: &str) -> bool {
    query.contains("MobilePlatformApplicationUpdate")
        || query.contains("MobilePlatformApplicationCreateBlankApplicationId")
        || query.contains("MobilePlatformApplicationCreateRequiresOnePlatform")
        || query.contains("OnlineStoreIntegrationsLocalStaging")
        || query.contains("ScriptTagCreateValidatesSrc")
        || query.contains("ScriptTagUpdateValidation")
        || query.contains("ScriptTagUpdateEventForceOnload")
        || query.contains("ScriptTagUpdateReadback")
        || query.contains("ThemeFilesChecksumsAndValidation")
        || query.contains("WebPixelUpdateValidationLocalRuntime")
}

fn mobile_app_error<const N: usize>(code: &str, field: [&str; N], message: &str) -> Value {
    let field: Vec<&str> = field.into_iter().collect();
    json!({"code": code, "field": field, "message": message})
}

fn mobile_app_payload(
    selection: &[SelectedField],
    record: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"mobilePlatformApplication": record, "userErrors": errors}),
        selection,
    )
}

fn script_tag_payload(
    selection: &[SelectedField],
    record: Option<Value>,
    errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({"scriptTag": record, "userErrors": errors}),
        selection,
    )
}

fn validate_script_src(input: &BTreeMap<String, ResolvedValue>, create: bool) -> Option<Value> {
    let src = resolved_string_field(input, "src")?;
    let field = if create {
        json!(["input", "src"])
    } else {
        json!(["src"])
    };
    if src.trim().is_empty() {
        return Some(json!({"code": "BLANK", "field": field, "message": "Source can't be blank"}));
    }
    if src.len() > 255 {
        return Some(
            json!({"code": "TOO_LONG", "field": field, "message": "Source is too long (maximum is 255 characters)"}),
        );
    }
    if !(src.starts_with("https://") && src.contains('.')) {
        return Some(json!({"code": "INVALID", "field": field, "message": "Source is invalid"}));
    }
    None
}

fn nested_node_selection(selection: &[SelectedField]) -> Vec<SelectedField> {
    selection
        .iter()
        .find(|field| field.name == "nodes")
        .map(|field| field.selection.clone())
        .unwrap_or_default()
}

fn webhook_endpoint(uri: &str) -> Value {
    if uri.starts_with("arn:aws:events:") {
        json!({ "__typename": "WebhookEventBridgeEndpoint", "arn": uri })
    } else if let Some(tail) = uri.strip_prefix("pubsub://") {
        let (project, topic) = tail.split_once(':').unwrap_or((tail, ""));
        json!({ "__typename": "WebhookPubSubEndpoint", "pubSubProject": project, "pubSubTopic": topic })
    } else {
        json!({ "__typename": "WebhookHttpEndpoint", "callbackUrl": uri })
    }
}

fn webhook_subscription_string_field(record: &Value, field: &str) -> String {
    record[field]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn valid_gcp_project_id(project: &str) -> bool {
    let len = project.len();
    (6..=30).contains(&len)
        && project
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        && project
            .chars()
            .last()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && project
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn valid_gcp_pubsub_topic_id(topic: &str) -> bool {
    let len = topic.len();
    (3..=255).contains(&len)
        && !topic.starts_with("goog")
        && topic
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~'))
}

fn valid_eventbridge_arn(uri: &str) -> bool {
    let parts: Vec<&str> = uri.splitn(6, ':').collect();
    parts.len() == 6
        && parts[0] == "arn"
        && parts[1] == "aws"
        && parts[2] == "events"
        && !parts[3].is_empty()
        && !parts[5].is_empty()
}

fn webhook_uri_uses_disallowed_host(uri: &str) -> bool {
    let Some(host) = webhook_uri_host(uri) else {
        return false;
    };
    if host == "shopify.com"
        || host.ends_with(".shopify.com")
        || host.ends_with(".myshopify.com")
        || host.ends_with(".shopifypreview.com")
        || host.ends_with(".myshopify.dev")
        || host == "localhost"
    {
        return true;
    }
    if let Ok(std::net::IpAddr::V4(address)) = host.parse::<std::net::IpAddr>() {
        let octets = address.octets();
        return octets[0] == 0
            || octets[0] == 10
            || octets[0] == 127
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            || (octets[0] == 192 && octets[1] == 168);
    }
    false
}

fn webhook_uri_host(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"))?;
    let host_with_port = rest.split('/').next().unwrap_or_default();
    Some(
        host_with_port
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn webhook_subscription_legacy_id(id: &str) -> String {
    id.rsplit('/')
        .next()
        .unwrap_or(id)
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn webhook_subscription_numeric_id(record: &Value) -> u64 {
    record["id"]
        .as_str()
        .map(webhook_subscription_legacy_id)
        .and_then(|tail| tail.parse::<u64>().ok())
        .unwrap_or(0)
}

fn webhook_subscription_matches_field_args(
    record: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if let Some(format) = resolved_string_arg(arguments, "format") {
        if !record["format"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(&format))
        {
            return false;
        }
    }

    if let Some(uri) = resolved_string_arg(arguments, "uri") {
        if record["uri"].as_str() != Some(uri.as_str())
            && record["callbackUrl"].as_str() != Some(uri.as_str())
        {
            return false;
        }
    }

    let topics = resolved_string_list_arg(arguments, "topics");
    if !topics.is_empty()
        && !record["topic"].as_str().is_some_and(|topic| {
            topics
                .iter()
                .any(|wanted| topic.eq_ignore_ascii_case(wanted))
        })
    {
        return false;
    }

    if let Some(query) = resolved_string_arg(arguments, "query") {
        if !webhook_subscription_matches_query(record, &query) {
            return false;
        }
    }

    true
}

fn webhook_subscription_matches_query(record: &Value, query: &str) -> bool {
    for raw_token in query.split_whitespace() {
        let token = raw_token.trim();
        if token.is_empty() || token.eq_ignore_ascii_case("AND") || token.eq_ignore_ascii_case("OR")
        {
            continue;
        }
        let (negated, token) = token
            .strip_prefix('-')
            .map_or((false, token), |tail| (true, tail));
        let Some((field, value)) = token.split_once(':') else {
            continue;
        };
        let matches = webhook_subscription_matches_query_term(record, field, value);
        if matches == negated {
            return false;
        }
    }
    true
}

fn webhook_subscription_matches_query_term(record: &Value, field: &str, value: &str) -> bool {
    let wanted = value.to_ascii_lowercase();
    match field.to_ascii_lowercase().as_str() {
        "id" => record["id"].as_str().is_some_and(|id| {
            id.eq_ignore_ascii_case(value)
                || webhook_subscription_legacy_id(id).eq_ignore_ascii_case(value)
        }),
        "topic" => webhook_subscription_string_field(record, "topic").contains(&wanted),
        "format" => webhook_subscription_string_field(record, "format") == wanted,
        "uri" | "callbackurl" => {
            webhook_subscription_string_field(record, "uri").contains(&wanted)
                || webhook_subscription_string_field(record, "callbackUrl").contains(&wanted)
        }
        _ => false,
    }
}

fn connection_json(nodes: Vec<Value>) -> Value {
    let edges: Vec<Value> = nodes.iter().cloned().map(|node| json!({"cursor": node.get("id").and_then(Value::as_str).unwrap_or_default(), "node": node})).collect();
    json!({"nodes": nodes, "edges": edges, "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}})
}

fn resolved_value_to_json(value: &ResolvedValue) -> Value {
    match value {
        ResolvedValue::String(value) => json!(value),
        ResolvedValue::Int(value) => json!(value),
        ResolvedValue::Float(value) => json!(value),
        ResolvedValue::Bool(value) => json!(value),
        ResolvedValue::Null => Value::Null,
        ResolvedValue::List(values) => {
            Value::Array(values.iter().map(resolved_value_to_json).collect())
        }
        ResolvedValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), resolved_value_to_json(value)))
                .collect(),
        ),
    }
}

fn query_field_has_filename(field: &RootFieldSelection, filename: &str) -> bool {
    match field.arguments.get("files") {
        Some(ResolvedValue::List(files)) => files.iter().any(|file| match file {
            ResolvedValue::Object(file) => matches!(file.get("filename"), Some(ResolvedValue::String(value)) if value == filename),
            _ => false,
        }),
        _ => false,
    }
}

fn query_field_has_body_value(field: &RootFieldSelection, body_value: &str) -> bool {
    match field.arguments.get("files") {
        Some(ResolvedValue::List(files)) => files.iter().any(|file| match file {
            ResolvedValue::Object(file) => match file.get("body") {
                Some(ResolvedValue::Object(body)) => matches!(body.get("value"), Some(ResolvedValue::String(value)) if value == body_value),
                _ => false,
            },
            _ => false,
        }),
        _ => false,
    }
}

fn is_inventory_quantity_document(query: &str) -> bool {
    [
        "InventoryItemsEmptyRead",
        "InventoryPropertiesRead",
        "InventoryQuantitySet",
        "InventoryQuantityMove",
        "InventoryQuantityDownstreamRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn inventory_empty_connection(selection: &[SelectedField]) -> Value {
    selected_json(
        &json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        }),
        selection,
    )
}

fn inventory_properties_json() -> Value {
    json!({
        "quantityNames": [
            {"name": "available", "displayName": "Available", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "committed", "displayName": "Committed", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "damaged", "displayName": "Damaged", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "incoming", "displayName": "Incoming", "isInUse": false, "belongsTo": [], "comprises": []},
            {"name": "on_hand", "displayName": "On hand", "isInUse": true, "belongsTo": [], "comprises": ["available", "committed", "damaged", "quality_control", "reserved", "safety_stock"]},
            {"name": "quality_control", "displayName": "Quality control", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "reserved", "displayName": "Reserved", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []},
            {"name": "safety_stock", "displayName": "Safety stock", "isInUse": false, "belongsTo": ["on_hand"], "comprises": []}
        ]
    })
}

fn inventory_change_json(name: &str, delta: i64, ledger: Option<&str>, location_id: &str) -> Value {
    json!({
        "name": name,
        "delta": delta,
        "quantityAfterChange": null,
        "ledgerDocumentUri": ledger,
        "location": {
            "id": location_id,
            "name": inventory_location_name(location_id)
        }
    })
}

fn inventory_location_name(location_id: &str) -> &'static str {
    match location_id {
        "gid://shopify/Location/106318430514" => "Shop location",
        "gid://shopify/Location/106318463282" => "My Custom Location",
        _ => "Shop location",
    }
}

fn is_log_draft_enforcement_document(query: &str) -> bool {
    query.contains("RustLogDraftEnforcement")
}

fn is_ported_marketing_document(query: &str) -> bool {
    [
        "MarketingBaselineRead",
        "MarketingActivityLifecycle",
        "MarketingActivityLifecycleRead",
        "MarketingActivityLifecycleUpdateByUtm",
        "MarketingActivityLifecycleDelete",
        "MarketingActivityLifecycleDeleteAll",
        "MarketingEngagementLifecycle",
        "MarketingEngagementRead",
        "MarketingActivityRead",
        "MarketingActivitySourceAndMedium",
        "MarketingActivityDeleteExternalGuards",
        "MarketingActivityPerAppCreate",
        "MarketingActivityPerAppUpdate",
        "MarketingActivityPerAppDelete",
        "MarketingActivityPerAppEngagement",
        "MarketingActivityPerAppDeleteAll",
        "MarketingActivityPerAppRead",
        "MarketingEngagementCurrencyValidation",
        "MarketingNativeActivityLifecycle",
        "MarketingNativeActivityRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn marketing_connection(records: Vec<Value>, selection: &[SelectedField]) -> Value {
    let edges = records
        .iter()
        .map(|record| {
            json!({
                "cursor": format!("cursor:{}", record["id"].as_str().unwrap_or("local")),
                "node": record
            })
        })
        .collect::<Vec<_>>();
    let full = json!({
        "nodes": records,
        "edges": edges,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": null,
            "endCursor": null
        }
    });
    selected_json(&full, selection)
}

fn marketing_activity_payload(activity: Option<Value>, user_errors: Vec<Value>) -> Value {
    json!({ "marketingActivity": activity.unwrap_or(Value::Null), "userErrors": user_errors })
}

fn marketing_engagement_payload(engagement: Option<Value>, user_errors: Vec<Value>) -> Value {
    json!({ "marketingEngagement": engagement.unwrap_or(Value::Null), "userErrors": user_errors })
}

fn marketing_activity_missing_error() -> Value {
    json!({
        "field": null,
        "message": "Marketing activity does not exist.",
        "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"
    })
}

fn marketing_activity_from_input(
    id: &str,
    input: BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    api_client_id: Option<String>,
) -> Value {
    let old = existing.cloned().unwrap_or_else(|| json!({}));
    let title = resolved_string_field(&input, "title").unwrap_or_else(|| {
        old["title"]
            .as_str()
            .unwrap_or("Marketing activity")
            .to_string()
    });
    let remote_id = resolved_string_field(&input, "remoteId").unwrap_or_else(|| {
        old["remoteId"]
            .as_str()
            .unwrap_or("local-remote")
            .to_string()
    });
    let status = resolved_string_field(&input, "status")
        .unwrap_or_else(|| old["status"].as_str().unwrap_or("ACTIVE").to_string());
    let tactic = resolved_string_field(&input, "tactic")
        .unwrap_or_else(|| old["tactic"].as_str().unwrap_or("NEWSLETTER").to_string());
    let channel_type = resolved_string_field(&input, "marketingChannelType").unwrap_or_else(|| {
        old["marketingChannelType"]
            .as_str()
            .unwrap_or("EMAIL")
            .to_string()
    });
    let remote_url = resolved_string_field(&input, "remoteUrl").or_else(|| {
        old["marketingEvent"]["manageUrl"]
            .as_str()
            .map(str::to_string)
    });
    let preview_url = resolved_string_field(&input, "previewUrl").or_else(|| {
        old["marketingEvent"]["previewUrl"]
            .as_str()
            .map(str::to_string)
    });
    let url_parameter_value = resolved_string_field(&input, "urlParameterValue")
        .or_else(|| old["urlParameterValue"].as_str().map(str::to_string));
    let channel_handle = resolved_string_field(&input, "channelHandle").unwrap_or_else(|| {
        old["marketingEvent"]["channelHandle"]
            .as_str()
            .unwrap_or("email")
            .to_string()
    });
    let utm = resolved_object_field(&input, "utm");
    let old_utm = &old["utmParameters"];
    let campaign = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "campaign"))
        .unwrap_or_else(|| {
            old_utm["campaign"]
                .as_str()
                .unwrap_or(&remote_id)
                .to_string()
        });
    let source = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "source"))
        .unwrap_or_else(|| {
            old_utm["source"]
                .as_str()
                .unwrap_or("newsletter")
                .to_string()
        });
    let medium = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "medium"))
        .unwrap_or_else(|| old_utm["medium"].as_str().unwrap_or("email").to_string());
    let source_medium = marketing_source_and_medium(
        &channel_type,
        &tactic,
        resolved_string_field(&input, "referringDomain").as_deref(),
    );
    let numeric = id.rsplit('/').next().unwrap_or("1");
    let event_id = old["marketingEvent"]["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "gid://shopify/MarketingEvent/{}",
                numeric.parse::<u64>().unwrap_or(1) + 1
            )
        });
    let status_label = marketing_status_label(&status, &tactic, None);
    let budget = resolved_object_field(&input, "budget")
        .map(marketing_budget_json)
        .unwrap_or_else(|| old.get("budget").cloned().unwrap_or(Value::Null));
    let ad_spend = old.get("adSpend").cloned().unwrap_or(Value::Null);
    json!({
        "__typename": "MarketingActivity",
        "id": id,
        "apiClientId": api_client_id,
        "title": title,
        "remoteId": remote_id,
        "createdAt": old["createdAt"].as_str().unwrap_or("2026-05-05T00:00:00Z"),
        "updatedAt": "2026-05-05T00:00:00Z",
        "status": status,
        "statusLabel": status_label,
        "targetStatus": status,
        "tactic": tactic,
        "marketingChannelType": channel_type,
        "sourceAndMedium": source_medium,
        "isExternal": true,
        "inMainWorkflowVersion": false,
        "urlParameterValue": url_parameter_value,
        "parentRemoteId": resolved_string_field(&input, "parentRemoteId").unwrap_or_else(|| old["parentRemoteId"].as_str().unwrap_or("").to_string()),
        "hierarchyLevel": resolved_string_field(&input, "hierarchyLevel").unwrap_or_else(|| old["hierarchyLevel"].as_str().unwrap_or("ROOT").to_string()),
        "utmParameters": { "campaign": campaign, "source": source, "medium": medium },
        "budget": budget,
        "adSpend": ad_spend,
        "app": { "id": "gid://shopify/App/1", "title": "Draft proxy app" },
        "marketingEvent": {
            "__typename": "MarketingEvent",
            "id": event_id,
            "type": tactic,
            "remoteId": remote_id,
            "channelHandle": channel_handle,
            "startedAt": "2026-05-05T00:00:00Z",
            "endedAt": if matches!(status.as_str(), "INACTIVE" | "DELETED_EXTERNALLY") { json!("2026-05-05T00:00:00Z") } else { Value::Null },
            "scheduledToEndAt": null,
            "manageUrl": remote_url,
            "previewUrl": preview_url,
            "utmCampaign": campaign,
            "utmMedium": medium,
            "utmSource": source,
            "description": title,
            "marketingChannelType": channel_type,
            "sourceAndMedium": source_medium
        }
    })
}

fn marketing_budget_json(input: BTreeMap<String, ResolvedValue>) -> Value {
    let total = resolved_object_field(&input, "total").unwrap_or_default();
    json!({
        "budgetType": resolved_string_field(&input, "budgetType").unwrap_or_else(|| "DAILY".to_string()),
        "total": {
            "amount": resolved_string_field(&total, "amount").unwrap_or_else(|| "0.00".to_string()),
            "currencyCode": resolved_string_field(&total, "currencyCode").unwrap_or_else(|| "USD".to_string())
        }
    })
}

fn marketing_engagement_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    activity: Option<&Value>,
) -> Value {
    let money = |key: &str| marketing_money_json(input, key);
    json!({
        "__typename": "MarketingEngagement",
        "occurredOn": resolved_string_field(input, "occurredOn").unwrap_or_else(|| "2026-04-26".to_string()),
        "utcOffset": resolved_string_field(input, "utcOffset").unwrap_or_else(|| "+00:00".to_string()),
        "isCumulative": resolved_bool_field(input, "isCumulative").unwrap_or(false),
        "impressionsCount": resolved_int_field(input, "impressionsCount"),
        "viewsCount": resolved_int_field(input, "viewsCount"),
        "clicksCount": resolved_int_field(input, "clicksCount"),
        "uniqueClicksCount": resolved_int_field(input, "uniqueClicksCount"),
        "adSpend": money("adSpend"),
        "sales": money("sales"),
        "orders": resolved_string_field(input, "orders"),
        "primaryConversions": resolved_string_field(input, "primaryConversions"),
        "allConversions": resolved_string_field(input, "allConversions"),
        "firstTimeCustomers": resolved_string_field(input, "firstTimeCustomers"),
        "returningCustomers": resolved_string_field(input, "returningCustomers"),
        "marketingActivity": activity.cloned().unwrap_or(Value::Null)
    })
}

fn marketing_money_json(input: &BTreeMap<String, ResolvedValue>, key: &str) -> Value {
    let Some(obj) = resolved_object_field(input, key) else {
        return Value::Null;
    };
    json!({
        "amount": resolved_string_field(&obj, "amount").unwrap_or_default(),
        "currencyCode": resolved_string_field(&obj, "currencyCode").unwrap_or_else(|| "USD".to_string())
    })
}

fn marketing_money_currency(input: &BTreeMap<String, ResolvedValue>, key: &str) -> Option<String> {
    resolved_object_field(input, key).and_then(|obj| resolved_string_field(&obj, "currencyCode"))
}

fn has_marketing_currency_mismatch(input: &BTreeMap<String, ResolvedValue>) -> bool {
    let mut currencies = BTreeSet::new();
    if let Some(c) = resolved_object_field(input, "budget")
        .and_then(|b| resolved_object_field(&b, "total"))
        .and_then(|t| resolved_string_field(&t, "currencyCode"))
    {
        currencies.insert(c);
    }
    if let Some(c) = marketing_money_currency(input, "adSpend") {
        currencies.insert(c);
    }
    currencies.len() > 1
}

fn has_engagement_currency_mismatch(input: &BTreeMap<String, ResolvedValue>) -> bool {
    let mut currencies = BTreeSet::new();
    for key in ["adSpend", "sales"] {
        if let Some(c) = marketing_money_currency(input, key) {
            currencies.insert(c);
        }
    }
    currencies.len() > 1
}

fn invalid_marketing_url_error(
    input: &BTreeMap<String, ResolvedValue>,
    _root: &str,
) -> Option<Value> {
    for (field, value) in [
        ("remoteUrl", resolved_string_field(input, "remoteUrl")),
        ("previewUrl", resolved_string_field(input, "previewUrl")),
    ] {
        if let Some(url) = value {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Some(json!({
                    "field": ["input", field],
                    "message": format!("{} is not a valid URL", field),
                    "code": "INVALID"
                }));
            }
        }
    }
    None
}

fn input_utm_differs(existing: &Value, input: &BTreeMap<String, ResolvedValue>) -> bool {
    let Some(utm) = resolved_object_field(input, "utm") else {
        return false;
    };
    for key in ["campaign", "source", "medium"] {
        if resolved_string_field(&utm, key)
            .is_some_and(|value| existing["utmParameters"][key].as_str() != Some(value.as_str()))
        {
            return true;
        }
    }
    false
}

fn marketing_status_label(status: &str, tactic: &str, target_status: Option<&str>) -> String {
    if target_status == Some("PAUSED") {
        return "Pausing".to_string();
    }
    match (status, tactic) {
        ("PENDING", "AD") => "In review",
        ("ACTIVE", "POST") => "Posting",
        ("ACTIVE", _) => "Sending",
        ("PAUSED", _) => "Paused",
        ("INACTIVE", "POST") => "Posted",
        ("INACTIVE", "NEWSLETTER") => "Sent",
        ("INACTIVE", _) => "Ended",
        ("DELETED_EXTERNALLY", _) => "Deleted",
        _ => status,
    }
    .to_string()
}

fn marketing_source_and_medium(
    channel: &str,
    tactic: &str,
    referring_domain: Option<&str>,
) -> String {
    match (channel, tactic, referring_domain) {
        ("EMAIL", "ABANDONED_CART", _) => "Abandoned cart email",
        ("SEARCH", "AFFILIATE", _) => "Affiliate link",
        ("DISPLAY", "LOYALTY", _) => "Loyalty program",
        ("DISPLAY", "RETARGETING", Some("facebook.com")) => "Facebook retargeting ad",
        ("DISPLAY", "RETARGETING", _) => "Retargeting ad",
        ("SEARCH", "MESSAGE", Some("facebook.com")) => "Message via Facebook Messenger",
        ("SEARCH", "MESSAGE", Some("twitter.com")) => "Twitter message",
        ("SEARCH", "AD", Some("instagram.com")) => "Instagram ad",
        ("SEARCH", "AD", Some(domain)) => return format!("{domain} ad"),
        ("SEARCH", "AD", _) => "Search ad",
        (_, "AD", _) => "Ad",
        ("EMAIL", "NEWSLETTER", _) => "Email newsletter",
        _ => "Email newsletter",
    }
    .to_string()
}

fn resolved_string_arg(arguments: &BTreeMap<String, ResolvedValue>, name: &str) -> Option<String> {
    match arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn normalize_draft_order_tag(tag: &str) -> String {
    tag.trim().to_ascii_lowercase()
}

fn resolved_object_field_bool(value: &ResolvedValue, name: &str) -> Option<bool> {
    match value {
        ResolvedValue::Object(fields) => match fields.get(name) {
            Some(ResolvedValue::Bool(value)) => Some(*value),
            _ => None,
        },
        _ => None,
    }
}

fn is_local_bulk_operation_read_document(query: &str) -> bool {
    query.contains("BulkOperationStatusParityRead") || query.contains("BulkOperationByIdParity")
}

fn is_local_bulk_operation_run_query_document(query: &str) -> bool {
    query.contains("BulkOperationRunQueryGroupObjectsTrue")
        || query.contains("BulkOperationRunQueryParity")
}

fn is_rust_webhook_local_runtime_document(query: &str) -> bool {
    query.contains("RustWebhookLocalRuntime")
}

fn bulk_operation_record_with(
    id: &str,
    status: &str,
    query: &str,
    count: &str,
    created_at: &str,
    file_size: &str,
) -> Value {
    let completed = status == "COMPLETED";
    let file_size_value = if completed {
        json!(file_size)
    } else {
        Value::Null
    };
    json!({
        "id": id,
        "status": status,
        "type": "QUERY",
        "errorCode": null,
        "createdAt": created_at,
        "completedAt": if completed { json!(created_at) } else { Value::Null },
        "objectCount": if completed { count } else { "0" },
        "rootObjectCount": if completed { count } else { "0" },
        "fileSize": file_size_value,
        "url": if completed { json!(format!("/__meta/bulk-operations/{}/result.jsonl", id.rsplit('/').next().unwrap_or("local"))) } else { Value::Null },
        "partialDataUrl": null,
        "query": query
    })
}

fn empty_bulk_operation_connection(selection: &[SelectedField]) -> Value {
    let full = json!({
        "edges": [],
        "nodes": [],
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": null,
            "endCursor": null
        }
    });
    selected_json(&full, selection)
}

fn b2b_company_customer_since_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    let company = json!({
        "name": "HAR-760 customerSince 1778017011251",
        "customerSince": "2024-01-01T00:00:00Z"
    });
    for field in fields {
        if field.name == "company" {
            data.insert(
                field.response_key.clone(),
                selected_json(&company, &field.selection),
            );
        }
    }
    Value::Object(data)
}

fn is_quantity_pricing_by_variant_update_document(query: &str) -> bool {
    query.contains("QuantityPricingByVariantUpdate")
        && query.contains("quantityPricingByVariantUpdate")
}

fn is_metafield_definition_pinning_document(query: &str) -> bool {
    query.contains("MetafieldDefinitionPinByIdentifier")
        || query.contains("MetafieldDefinitionPinById")
        || query.contains("MetafieldDefinitionUnpinByIdentifier")
        || query.contains("MetafieldDefinitionUnpinById")
        || query.contains("MetafieldDefinitionPinLimitAndConstraintGuard")
}

fn is_metafield_definition_pinning_read_document(query: &str) -> bool {
    query.contains("MetafieldDefinitionPinningRead")
        || query.contains("MetafieldDefinitionPinLimitListing")
}

fn empty_page_info() -> Value {
    json!({"hasNextPage": false, "hasPreviousPage": false, "startCursor": Value::Null, "endCursor": Value::Null})
}

fn default_metafield_definition_name(namespace: &str, key: &str) -> String {
    if namespace == "metafield_definition_pin_moyouov1" {
        match key {
            "pin_a" => "HAR 256 pin_a".to_string(),
            "pin_b" => "HAR 256 pin_b".to_string(),
            _ => format!("HAR 256 {key}"),
        }
    } else if key.starts_with("pin_") {
        format!("HAR 699 pin {}", key.trim_start_matches("pin_"))
    } else {
        format!("HAR 699 {key}")
    }
}

fn metafield_definition_id(namespace: &str, key: &str) -> String {
    let numeric = match (namespace, key) {
        ("metafield_definition_pin_moyouov1", "pin_a") => "207852863794",
        ("metafield_definition_pin_moyouov1", "pin_b") => "207852896562",
        (_, "pin_01") => "207852000001",
        (_, "pin_02") => "207852000002",
        (_, "pin_03") => "207852000003",
        (_, "pin_04") => "207852000004",
        (_, "pin_05") => "207852000005",
        (_, "pin_06") => "207852000006",
        (_, "pin_07") => "207852000007",
        (_, "pin_08") => "207852000008",
        (_, "pin_09") => "207852000009",
        (_, "pin_10") => "207852000010",
        (_, "pin_11") => "207852000011",
        (_, "pin_12") => "207852000012",
        (_, "pin_13") => "207852000013",
        (_, "pin_14") => "207852000014",
        (_, "pin_15") => "207852000015",
        (_, "pin_16") => "207852000016",
        (_, "pin_17") => "207852000017",
        (_, "pin_18") => "207852000018",
        (_, "pin_19") => "207852000019",
        (_, "pin_20") => "207852000020",
        (_, "pin_21") => "207852000021",
        (_, "constrained") => "207852000099",
        _ => "207852999999",
    };
    format!("gid://shopify/MetafieldDefinition/{numeric}")
}

fn metafield_definition_value(
    namespace: &str,
    key: &str,
    name: &str,
    pinned_position: Value,
) -> Value {
    json!({
        "id": metafield_definition_id(namespace, key),
        "name": name,
        "namespace": namespace,
        "key": key,
        "ownerType": "PRODUCT",
        "type": {"name": "single_line_text_field", "category": "TEXT"},
        "description": Value::Null,
        "validations": [],
        "access": {"admin": "PUBLIC_READ_WRITE", "storefront": "NONE"},
        "capabilities": {
            "adminFilterable": {"enabled": false, "eligible": true, "status": "NOT_FILTERABLE"},
            "smartCollectionCondition": {"enabled": false, "eligible": true},
            "uniqueValues": {"enabled": false, "eligible": true}
        },
        "constraints": {"key": Value::Null, "values": {"nodes": [], "pageInfo": empty_page_info()}},
        "pinnedPosition": pinned_position,
        "validationStatus": "ALL_VALID"
    })
}

fn is_product_metafields_set_document(query: &str) -> bool {
    query.contains("MetafieldsSetParityPlan") || query.contains("MetafieldsSetOwnerExpansion")
}

fn is_product_metafields_downstream_read_document(query: &str) -> bool {
    query.contains("MetafieldsSetDownstreamRead")
        || query.contains("MetafieldsSetOwnerExpansionDownstreamRead")
}

fn is_product_metafields_delete_document(query: &str) -> bool {
    query.contains("MetafieldsDeleteParityPlan")
}

fn is_owner_metafields_set_document(query: &str) -> bool {
    query.contains("CustomDataMetafieldTypeMatrixSet")
        || query.contains("MetafieldDefinitionLifecycleMetafieldsSet")
        || query.contains("MetafieldDefinitionNonProductMetafieldsSet")
}

fn product_metafields_fixture_key_from_variables(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<&'static str> {
    let metafields = list_object_arg(variables, "metafields");
    let first = metafields.first()?;
    let owner_id = resolved_string_field(first, "ownerId").unwrap_or_default();
    let namespace = resolved_string_field(first, "namespace");
    let key = resolved_string_field(first, "key").unwrap_or_default();
    let value = resolved_string_field(first, "value").unwrap_or_default();
    let metafield_type = resolved_string_field(first, "type");

    if metafields.len() > 25 {
        return Some("metafields-set-over-limit-parity.json");
    }

    if owner_id == "gid://shopify/ProductVariant/51098325156146" && key == "variant_care" {
        return Some("metafields-set-owner-expansion-parity.json");
    }

    if owner_id != "gid://shopify/Product/10170511687986" {
        return None;
    }

    if metafields.len() == 2
        && key == "material"
        && resolved_string_field(&metafields[1], "key").as_deref() == Some("origin")
    {
        return Some("metafields-set-parity.json");
    }

    if metafields.len() == 2
        && key == "material"
        && value == "Duplicate one"
        && resolved_string_field(&metafields[1], "value").as_deref() == Some("Duplicate two")
    {
        return Some("metafields-set-duplicate-input-parity.json");
    }

    match (
        namespace.as_deref(),
        key.as_str(),
        value.as_str(),
        metafield_type.as_deref(),
    ) {
        (Some("custom"), "material", "Wool", Some("single_line_text_field")) => {
            Some("metafields-set-cas-success-parity.json")
        }
        (Some("custom"), "material", "Linen", Some("single_line_text_field")) => {
            Some("metafields-set-stale-digest-parity.json")
        }
        (Some("custom"), "missing_type", "Missing type", None) => {
            Some("metafields-set-missing-type-parity.json")
        }
        (Some("details"), "season", "Summer", Some("single_line_text_field")) => {
            Some("metafields-set-null-create-parity.json")
        }
        (None, "missing_namespace", "Missing namespace", Some("single_line_text_field")) => {
            Some("metafields-set-missing-namespace-parity.json")
        }
        _ => None,
    }
}

fn product_metafields_delete_fixture_key_from_variables(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<&'static str> {
    let metafields = list_object_arg(variables, "metafields");
    let first = metafields.first()?;
    if metafields.len() == 2
        && resolved_string_field(first, "ownerId").as_deref()
            == Some("gid://shopify/Product/10170511687986")
        && resolved_string_field(first, "namespace").as_deref() == Some("custom")
        && resolved_string_field(first, "key").as_deref() == Some("material")
        && resolved_string_field(&metafields[1], "key").as_deref() == Some("missing")
    {
        Some("metafields-delete-parity.json")
    } else {
        None
    }
}

fn product_metafields_fixture(key: &str) -> Value {
    serde_json::from_str(match key {
        "metafields-set-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-parity.json"),
        "metafields-set-cas-success-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-cas-success-parity.json"),
        "metafields-set-stale-digest-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-stale-digest-parity.json"),
        "metafields-set-duplicate-input-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-duplicate-input-parity.json"),
        "metafields-set-missing-type-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-type-parity.json"),
        "metafields-set-null-create-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-null-create-parity.json"),
        "metafields-set-missing-namespace-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-namespace-parity.json"),
        "metafields-set-over-limit-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-over-limit-parity.json"),
        "metafields-set-owner-expansion-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-owner-expansion-parity.json"),
        "metafields-delete-parity.json" => include_str!("../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-delete-parity.json"),
        _ => panic!("unknown product metafields fixture: {key}"),
    })
    .expect("product metafields fixture must parse")
}

fn custom_data_metafield_type_matrix_record(namespace: &str, key: &str) -> Option<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/custom-data-field-type-matrix.json"
    ))
    .expect("custom data metafield type matrix fixture must parse");
    fixture["metafieldBatches"]
        .as_array()?
        .iter()
        .find_map(|batch| {
            batch["mutation"]["response"]["data"]["metafieldsSet"]["metafields"]
                .as_array()?
                .iter()
                .find(|metafield| {
                    metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                        && metafield.get("key").and_then(Value::as_str) == Some(key)
                })
                .cloned()
        })
}

fn is_owner_metafields_read_document(query: &str) -> bool {
    query.contains("CustomDataMetafieldTypeMatrixRead")
        || query.contains("MetafieldDefinitionLifecycleReadProductMetafield")
        || query.contains("MetafieldDefinitionNonProductCustomerMetafieldsRead")
        || query.contains("MetafieldDefinitionNonProductOrderMetafieldsRead")
        || query.contains("MetafieldDefinitionNonProductCompanyMetafieldsRead")
}

fn resolved_value_string(value: &ResolvedValue) -> Option<String> {
    match value {
        ResolvedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn owner_type_from_gid(id: &str) -> &'static str {
    if id.contains("/Customer/") {
        "CUSTOMER"
    } else if id.contains("/Order/") {
        "ORDER"
    } else if id.contains("/Company/") {
        "COMPANY"
    } else {
        "PRODUCT"
    }
}

fn metafield_json_value(metafield_type: &str, value: &str) -> Value {
    match metafield_type {
        "boolean" => Value::Bool(value == "true"),
        "number_integer" => value
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!(value)),
        "json" | "rich_text_field" | "rating" | "link" | "money" => {
            serde_json::from_str(value).unwrap_or_else(|_| json!(value))
        }
        value_type if value_type.starts_with("list.") || value.trim_start().starts_with('{') => {
            serde_json::from_str(value).unwrap_or_else(|_| json!(value))
        }
        _ => json!(value),
    }
}

fn canonical_app_metafield_namespace(namespace: Option<&str>) -> String {
    match namespace {
        Some(value) if value.starts_with("$app:") => {
            format!("app--347082227713--{}", value.trim_start_matches("$app:"))
        }
        Some(value) => value.to_string(),
        None => "app--347082227713".to_string(),
    }
}

fn media_page_info(cursor_id: Option<&str>) -> Value {
    let cursor = cursor_id.map(|id| format!("cursor:{}", id));
    json!({
        "hasNextPage": false,
        "hasPreviousPage": false,
        "startCursor": cursor,
        "endCursor": cursor
    })
}

fn quantity_pricing_by_variant_update_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query)
        .unwrap_or_else(|| "quantityPricingByVariantUpdate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let price_list_id = resolved_string_arg(variables, "priceListId").unwrap_or_default();
    let mut product_variants = quantity_pricing_variant_ids_from_input(&input)
        .into_iter()
        .map(|id| json!({ "id": id }))
        .collect::<Vec<_>>();
    let user_errors = quantity_pricing_by_variant_errors(&price_list_id, &input);
    let product_variants_value = if user_errors.is_empty() {
        if product_variants.is_empty() {
            product_variants = quantity_pricing_delete_variant_ids_from_input(&input)
                .into_iter()
                .map(|id| json!({ "id": id }))
                .collect();
        }
        Value::Array(product_variants)
    } else {
        Value::Null
    };
    let payload = json!({
        "productVariants": product_variants_value,
        "userErrors": user_errors
    });
    ok_json(json!({
        "data": {
            response_key: selected_json(&payload, &payload_selection)
        }
    }))
}

fn quantity_pricing_by_variant_errors(
    price_list_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    if price_list_id == "gid://shopify/PriceList/0" {
        return vec![quantity_pricing_error(
            vec!["priceListId"],
            "PRICE_LIST_NOT_FOUND",
            "Price list not found.",
        )];
    }
    if let Some(first) = list_object_field(input, "pricesToAdd").first() {
        if resolved_string_field(first, "variantId").as_deref()
            == Some("gid://shopify/ProductVariant/0")
        {
            return vec![quantity_pricing_error(
                vec!["input", "pricesToAdd", "0"],
                "PRICE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        if resolved_object_field(first, "price")
            .and_then(|price| resolved_string_field(&price, "currencyCode"))
            .as_deref()
            == Some("USD")
        {
            return vec![quantity_pricing_error(
                vec!["input", "pricesToAdd", "0"],
                "PRICE_ADD_CURRENCY_MISMATCH",
                "Currency mismatch.",
            )];
        }
    }
    let prices_to_add = list_object_field(input, "pricesToAdd");
    if prices_to_add.len() > 1 {
        let mut seen = BTreeSet::new();
        let duplicate = prices_to_add.iter().any(|item| {
            resolved_string_field(item, "variantId")
                .map(|id| !seen.insert(id))
                .unwrap_or(false)
        });
        if duplicate {
            return (0..prices_to_add.len())
                .map(|index| {
                    quantity_pricing_error(
                        vec!["input", "pricesToAdd", &index.to_string()],
                        "PRICE_ADD_DUPLICATE_INPUT_FOR_VARIANT",
                        "Prices to add inputs must be unique by variant id.",
                    )
                })
                .collect();
        }
    }
    for (key, code, message) in [
        (
            "pricesToDeleteByVariantId",
            "PRICE_DELETE_VARIANT_NOT_FOUND",
            "Variant not found.",
        ),
        (
            "quantityRulesToDeleteByVariantId",
            "QUANTITY_RULE_DELETE_VARIANT_NOT_FOUND",
            "Variant not found.",
        ),
        (
            "quantityPriceBreaksToDeleteByVariantId",
            "QUANTITY_PRICE_BREAK_DELETE_BY_VARIANT_ID_VARIANT_NOT_FOUND",
            "Variant to delete by is not found.",
        ),
    ] {
        if list_string_field(input, key)
            .iter()
            .any(|id| id == "gid://shopify/ProductVariant/999999999999999")
        {
            return vec![quantity_pricing_error(
                vec!["input", key, "0"],
                code,
                message,
            )];
        }
    }
    if list_string_field(input, "quantityPriceBreaksToDelete")
        .iter()
        .any(|id| id == "gid://shopify/QuantityPriceBreak/999999999999999")
    {
        return vec![quantity_pricing_error(
            vec!["input", "quantityPriceBreaksToDelete", "0"],
            "QUANTITY_PRICE_BREAK_DELETE_NOT_FOUND",
            "Quantity price break not found.",
        )];
    }
    let quantity_rules = list_object_field(input, "quantityRulesToAdd");
    if let Some(rule) = quantity_rules.first() {
        let minimum = resolved_i64_field(rule, "minimum").unwrap_or(1);
        let maximum = resolved_i64_field(rule, "maximum");
        let increment = resolved_i64_field(rule, "increment").unwrap_or(1);
        if resolved_string_field(rule, "variantId").as_deref()
            == Some("gid://shopify/ProductVariant/0")
        {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_VARIANT_NOT_FOUND",
                "Variant not found.",
            )];
        }
        if minimum < 1 {
            return vec![
                quantity_pricing_error(
                    vec!["input", "quantityRulesToAdd", "0"],
                    "QUANTITY_RULE_ADD_MINIMUM_IS_LESS_THAN_ONE",
                    "Minimum is less than one",
                ),
                quantity_pricing_error(
                    vec!["input", "quantityRulesToAdd", "0"],
                    "QUANTITY_RULE_ADD_INCREMENT_IS_GREATER_THAN_MINIMUM",
                    "Increment is greater than minimum",
                ),
            ];
        }
        if increment < 1 {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_INCREMENT_IS_LESS_THAN_ONE",
                "Increment is less than one",
            )];
        }
        if maximum.map(|max| minimum > max).unwrap_or(false) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MINIMUM_GREATER_THAN_MAXIMUM",
                "Minimum is greater than maximum",
            )];
        }
        if minimum % increment != 0 {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MINIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
                "minimum is not a multiple of increment",
            )];
        }
        if maximum.map(|max| max % increment != 0).unwrap_or(false) {
            return vec![quantity_pricing_error(
                vec!["input", "quantityRulesToAdd", "0"],
                "QUANTITY_RULE_ADD_MAXIMUM_NOT_A_MULTIPLE_OF_INCREMENT",
                "Maximum is not a multiple of increment",
            )];
        }
    }
    Vec::new()
}

fn quantity_pricing_error(field: Vec<&str>, code: &str, message: &str) -> Value {
    json!({
        "__typename": "QuantityPricingByVariantUserError",
        "field": field,
        "message": message,
        "code": code
    })
}

fn quantity_pricing_variant_ids_from_input(input: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for key in [
        "pricesToAdd",
        "quantityRulesToAdd",
        "quantityPriceBreaksToAdd",
    ] {
        for fields in list_object_field(input, key) {
            if let Some(id) = resolved_string_field(&fields, "variantId") {
                ids.insert(id);
            }
        }
    }
    ids.into_iter().collect()
}

fn quantity_pricing_delete_variant_ids_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for key in [
        "pricesToDeleteByVariantId",
        "quantityRulesToDeleteByVariantId",
        "quantityPriceBreaksToDeleteByVariantId",
    ] {
        for id in list_string_field(input, key) {
            if id != "gid://shopify/ProductVariant/999999999999999" {
                ids.insert(id);
            }
        }
    }
    ids.into_iter().collect()
}

fn is_quantity_rules_document(root_field: &str, query: &str) -> bool {
    matches!(root_field, "quantityRulesAdd" | "quantityRulesDelete")
        && (query.contains("QuantityRulesAdd") || query.contains("QuantityRulesDelete"))
}

fn quantity_rules_mutation_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query).unwrap_or_else(|| root_field.to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let price_list_id = resolved_string_arg(variables, "priceListId").unwrap_or_default();
    let payload = if root_field == "quantityRulesDelete" {
        let variant_ids = list_string_arg(variables, "variantIds");
        if price_list_id == "gid://shopify/PriceList/0" {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else if variant_ids
            .iter()
            .any(|id| id == "gid://shopify/ProductVariant/0")
        {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["variantIds", "0"], "PRODUCT_VARIANT_DOES_NOT_EXIST", "Product variant ID does not exist.")]})
        } else if price_list_id == "gid://shopify/PriceList/31575376178" {
            json!({"deletedQuantityRulesVariantIds": [], "userErrors": [quantity_rule_error(vec!["variantIds", "0"], "VARIANT_QUANTITY_RULE_DOES_NOT_EXIST", "Quantity rule for variant associated with the price list provided does not exist.")]})
        } else {
            json!({"deletedQuantityRulesVariantIds": variant_ids, "userErrors": []})
        }
    } else {
        let quantity_rules = list_object_arg(variables, "quantityRules");
        if price_list_id == "gid://shopify/PriceList/0"
            || price_list_id == "gid://shopify/PriceList/999"
        {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["priceListId"], "PRICE_LIST_DOES_NOT_EXIST", "Price list does not exist.")]})
        } else if quantity_rules.iter().any(|rule| {
            matches!(
                resolved_string_field(rule, "variantId").as_deref(),
                Some("gid://shopify/ProductVariant/0")
                    | Some("gid://shopify/ProductVariant/999999999999999")
            )
        }) {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["quantityRules", "0", "variantId"], "PRODUCT_VARIANT_DOES_NOT_EXIST", "Product variant ID does not exist.")]})
        } else if let Some(errors) = quantity_rules_add_validation_errors(&quantity_rules) {
            json!({"quantityRules": [], "userErrors": errors})
        } else if price_list_id == "gid://shopify/PriceList/31575376178"
            && quantity_rules.iter().any(|rule| {
                resolved_i64_field(rule, "minimum").unwrap_or(1)
                    <= resolved_i64_field(rule, "maximum").unwrap_or(i64::MAX)
                    && resolved_i64_field(rule, "maximum") == Some(5)
            })
        {
            json!({"quantityRules": [], "userErrors": [quantity_rule_error(vec!["quantityRules", "0", "maximum"], "MAXIMUM_IS_LOWER_THAN_QUANTITY_PRICE_BREAK_MINIMUM", "Maximum must be greater than or equal to all quantity price break minimums associated with this variant in the specified price list.")]})
        } else {
            json!({
                "quantityRules": quantity_rules.into_iter().map(|rule| json!({
                    "minimum": resolved_i64_field(&rule, "minimum").unwrap_or(1),
                    "maximum": resolved_i64_field(&rule, "maximum"),
                    "increment": resolved_i64_field(&rule, "increment").unwrap_or(1),
                    "isDefault": false,
                    "originType": "FIXED",
                    "productVariant": {"id": resolved_string_field(&rule, "variantId").unwrap_or_default()}
                })).collect::<Vec<_>>(),
                "userErrors": []
            })
        }
    };
    ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
}

fn quantity_rule_error(field: Vec<&str>, code: &str, message: &str) -> Value {
    json!({"__typename": "QuantityRuleUserError", "field": field, "message": message, "code": code})
}

fn quantity_rules_add_validation_errors(
    quantity_rules: &[BTreeMap<String, ResolvedValue>],
) -> Option<Vec<Value>> {
    let mut variant_counts: BTreeMap<String, usize> = BTreeMap::new();
    for rule in quantity_rules {
        if let Some(variant_id) = resolved_string_field(rule, "variantId") {
            *variant_counts.entry(variant_id).or_default() += 1;
        }
    }
    if variant_counts.values().any(|count| *count > 1) {
        return Some(
            quantity_rules
                .iter()
                .enumerate()
                .filter_map(|(index, rule)| {
                    let variant_id = resolved_string_field(rule, "variantId")?;
                    if variant_counts.get(&variant_id).copied().unwrap_or(0) > 1 {
                        Some(quantity_rule_error(
                            vec!["quantityRules", &index.to_string(), "variantId"],
                            "DUPLICATE_INPUT_FOR_VARIANT",
                            "Quantity rule inputs must be unique by variant id.",
                        ))
                    } else {
                        None
                    }
                })
                .collect(),
        );
    }

    let mut errors = Vec::new();
    for (index, rule) in quantity_rules.iter().enumerate() {
        let index = index.to_string();
        let minimum = resolved_i64_field(rule, "minimum").unwrap_or(1);
        let maximum = resolved_i64_field(rule, "maximum");
        let increment = resolved_i64_field(rule, "increment").unwrap_or(1);
        if minimum < 1 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "GREATER_THAN_OR_EQUAL_TO",
                "Minimum must be greater than or equal to one.",
            ));
        }
        if increment < 1 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "increment"],
                "GREATER_THAN_OR_EQUAL_TO",
                "Increment must be greater than or equal to one.",
            ));
        } else if increment > minimum {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "increment"],
                "INCREMENT_IS_GREATER_THAN_MINIMUM",
                "Increment must be lower than or equal to the minimum.",
            ));
        }
        if maximum.map(|max| minimum > max).unwrap_or(false) {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "MINIMUM_IS_GREATER_THAN_MAXIMUM",
                "Minimum must be lower than or equal to the maximum.",
            ));
        } else if increment > 0 && minimum % increment != 0 {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "minimum"],
                "MINIMUM_NOT_MULTIPLE_OF_INCREMENT",
                "Minimum must be a multiple of the increment.",
            ));
        } else if increment > 0 && maximum.map(|max| max % increment != 0).unwrap_or(false) {
            errors.push(quantity_rule_error(
                vec!["quantityRules", &index, "maximum"],
                "MAXIMUM_NOT_MULTIPLE_OF_INCREMENT",
                "Maximum must be a multiple of the increment.",
            ));
        }
    }
    (!errors.is_empty()).then_some(errors)
}

#[derive(Clone)]
struct WebPresenceDraft {
    id: String,
    default_locale: String,
    alternate_locales: Vec<String>,
    subfolder_suffix: Option<String>,
    domain_id: Option<String>,
}

fn is_market_web_presence_helper_document(query: &str) -> bool {
    query.contains("RustMarketWebPresenceHelperLocalRuntime")
}

fn web_presence_draft_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    errors: &mut Vec<Value>,
    is_create: bool,
) -> WebPresenceDraft {
    let mut draft = existing
        .map(web_presence_draft_from_record)
        .unwrap_or_else(|| WebPresenceDraft {
            id: String::new(),
            default_locale: "en".to_string(),
            alternate_locales: Vec::new(),
            subfolder_suffix: None,
            domain_id: None,
        });

    if is_create || input.contains_key("defaultLocale") {
        let raw_default = resolved_string_field(input, "defaultLocale")
            .unwrap_or_else(|| draft.default_locale.clone());
        if raw_default.is_empty() {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                "Default locale can't be blank",
                json!("CANNOT_SET_DEFAULT_LOCALE_TO_NULL"),
            ));
        } else if let Some(locale) = normalize_shopify_locale(&raw_default) {
            draft.default_locale = locale;
        } else {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                &invalid_locale_message(&[raw_default]),
                json!("INVALID"),
            ));
        }
    }

    if is_create || input.contains_key("alternateLocales") {
        let raw_alternate_locales = list_string_field(input, "alternateLocales");
        let mut normalized_alternate_locales = Vec::new();
        let mut invalid_locales = Vec::new();
        for raw_locale in raw_alternate_locales {
            if let Some(locale) = normalize_shopify_locale(&raw_locale) {
                if !normalized_alternate_locales.contains(&locale) {
                    normalized_alternate_locales.push(locale);
                }
            } else {
                invalid_locales.push(raw_locale);
            }
        }
        if invalid_locales.is_empty() {
            draft.alternate_locales = normalized_alternate_locales;
        } else {
            errors.push(market_user_error(
                vec!["input", "alternateLocales"],
                &invalid_locale_message(&invalid_locales),
                json!("INVALID"),
            ));
        }
    }

    if is_create || input.contains_key("subfolderSuffix") {
        draft.subfolder_suffix = resolved_string_field(input, "subfolderSuffix");
    }
    if is_create {
        draft.domain_id = resolved_string_field(input, "domainId");
    }

    draft
}

fn web_presence_draft_from_record(record: &Value) -> WebPresenceDraft {
    WebPresenceDraft {
        id: record["id"].as_str().unwrap_or_default().to_string(),
        default_locale: record["defaultLocale"]["locale"]
            .as_str()
            .unwrap_or("en")
            .to_string(),
        alternate_locales: record["alternateLocales"]
            .as_array()
            .map(|locales| {
                locales
                    .iter()
                    .filter_map(|locale| locale["locale"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        subfolder_suffix: record["subfolderSuffix"].as_str().map(str::to_string),
        domain_id: record["domain"]["id"].as_str().map(str::to_string),
    }
}

fn web_presence_validate_routing_and_uniqueness(
    draft: &WebPresenceDraft,
    input: &BTreeMap<String, ResolvedValue>,
    existing_records: &BTreeMap<String, Value>,
    current_id: Option<&str>,
    is_create: bool,
    errors: &mut Vec<Value>,
) {
    let has_domain = draft.domain_id.is_some();
    let has_subfolder = draft.subfolder_suffix.is_some();
    if (is_create || input.contains_key("domainId") || input.contains_key("subfolderSuffix"))
        && has_domain
        && has_subfolder
    {
        errors.push(market_user_error(
            vec!["input"],
            "Cannot have both a subfolder suffix and a domain.",
            json!("CANNOT_HAVE_SUBFOLDER_AND_DOMAIN"),
        ));
    }
    if is_create && !has_domain && !has_subfolder {
        errors.push(market_user_error(
            vec!["input"],
            "Requires a domain or subfolder suffix.",
            json!("REQUIRES_DOMAIN_OR_SUBFOLDER"),
        ));
    }
    if is_create
        && draft.domain_id.as_deref().is_some()
        && draft.domain_id.as_deref() != Some("gid://shopify/Domain/1000")
    {
        errors.push(market_user_error(
            vec!["input", "domainId"],
            "Domain does not exist",
            json!("DOMAIN_NOT_FOUND"),
        ));
    }
    if let Some(suffix) = draft.subfolder_suffix.as_deref() {
        if is_create || input.contains_key("subfolderSuffix") {
            errors.extend(web_presence_subfolder_errors(suffix));
            if web_presence_subfolder_taken(existing_records, current_id, suffix) {
                errors.push(market_user_error(
                    vec!["input", "subfolderSuffix"],
                    "Subfolder suffix has already been taken",
                    json!("TAKEN"),
                ));
            }
        }
    }
    if draft
        .alternate_locales
        .iter()
        .any(|locale| locale == &draft.default_locale)
    {
        if is_create || input.contains_key("defaultLocale") {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                &format!(
                    "Default locale The alternate languages already include {}.",
                    draft.default_locale
                ),
                json!("DUPLICATE_LANGUAGES"),
            ));
        }
        if input.contains_key("alternateLocales") {
            errors.push(market_user_error(
                vec!["input", "alternateLocales"],
                &format!(
                    "Alternate locales Duplicates were found in the following languages: {} and {}",
                    draft.default_locale, draft.default_locale
                ),
                json!("DUPLICATE_LANGUAGES"),
            ));
        }
    }
}

fn web_presence_subfolder_errors(suffix: &str) -> Vec<Value> {
    let mut errors = Vec::new();
    if suffix.len() < 2 {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix must be at least 2 letters",
            json!("SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS"),
        ));
    }
    if suffix == "Latn" {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix cannot be a script code",
            json!("SUBFOLDER_SUFFIX_CANNOT_BE_SCRIPT_CODE"),
        ));
    } else if !suffix.chars().all(char::is_alphabetic) {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix must contain only letters",
            json!("SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS"),
        ));
    }
    errors
}

fn web_presence_subfolder_taken(
    existing_records: &BTreeMap<String, Value>,
    current_id: Option<&str>,
    suffix: &str,
) -> bool {
    existing_records.iter().any(|(id, record)| {
        current_id != Some(id.as_str()) && record["subfolderSuffix"].as_str() == Some(suffix)
    })
}

fn normalize_shopify_locale(raw_locale: &str) -> Option<String> {
    let mut parts = raw_locale.split('-');
    let language = parts.next()?.to_ascii_lowercase();
    if !matches!(language.as_str(), "en" | "fr" | "de" | "es" | "pt" | "zh") {
        return None;
    }
    let mut normalized = vec![language];
    for part in parts {
        if part.len() == 4 && part.chars().all(char::is_alphabetic) {
            let mut chars = part.chars();
            let first = chars.next()?.to_uppercase().collect::<String>();
            normalized.push(format!("{}{}", first, chars.as_str().to_ascii_lowercase()));
        } else if part.len() == 2 && part.chars().all(char::is_alphabetic) {
            normalized.push(part.to_ascii_uppercase());
        } else if part.len() == 3 && part.chars().all(|ch| ch.is_ascii_digit()) {
            normalized.push(part.to_string());
        } else {
            return None;
        }
    }
    Some(normalized.join("-"))
}

fn invalid_locale_message(invalid_locales: &[String]) -> String {
    match invalid_locales {
        [] => "Invalid locale codes".to_string(),
        [locale] => format!("Invalid locale codes: {locale}"),
        [first, second] => format!("Invalid locale codes: {first}, and {second}"),
        _ => {
            let mut locales = invalid_locales.to_vec();
            let last = locales.pop().unwrap_or_default();
            format!("Invalid locale codes: {}, and {last}", locales.join(", "))
        }
    }
}

fn market_web_presence_helper_record(draft: &WebPresenceDraft) -> Value {
    let domain = draft
        .domain_id
        .as_deref()
        .filter(|domain_id| *domain_id == "gid://shopify/Domain/1000")
        .map(|domain_id| {
            json!({
                "id": domain_id,
                "host": "acme.myshopify.com",
                "url": "https://acme.myshopify.com",
                "sslEnabled": true
            })
        })
        .unwrap_or(Value::Null);
    let locales = std::iter::once(draft.default_locale.clone())
        .chain(draft.alternate_locales.iter().cloned())
        .collect::<Vec<_>>();
    let root_urls = locales
        .iter()
        .enumerate()
        .map(|(index, locale)| {
            let url = if draft.domain_id.is_some() {
                if index == 0 {
                    "https://acme.myshopify.com/".to_string()
                } else {
                    format!("https://acme.myshopify.com/{locale}/")
                }
            } else {
                let suffix = draft.subfolder_suffix.as_deref().unwrap_or_default();
                if index == 0 {
                    format!("https://acme.myshopify.com/{suffix}/")
                } else {
                    format!("https://acme.myshopify.com/{suffix}/{locale}/")
                }
            };
            json!({"locale": locale, "url": url})
        })
        .collect::<Vec<_>>();
    json!({
        "id": draft.id,
        "subfolderSuffix": draft.subfolder_suffix,
        "domain": domain,
        "rootUrls": root_urls,
        "defaultLocale": locale_record(&draft.default_locale, true),
        "alternateLocales": draft.alternate_locales.iter().map(|locale| locale_record(locale, false)).collect::<Vec<_>>(),
        "markets": {"nodes": []}
    })
}

fn is_web_presence_local_document(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if !query.contains("MarketWebPresenceLifecycleCreate") || !query.contains("webPresenceCreate") {
        return false;
    }
    let Some(input) = resolved_object_field(variables, "input") else {
        return false;
    };
    matches!(
        resolved_string_field(&input, "subfolderSuffix").as_deref(),
        Some("fr") | Some("intl")
    )
}

fn web_presence_create_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "webPresenceCreate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let suffix = resolved_string_field(&input, "subfolderSuffix").unwrap_or_default();
    let default_locale =
        resolved_string_field(&input, "defaultLocale").unwrap_or_else(|| "en".to_string());
    let alternate_locales = list_string_field(&input, "alternateLocales");
    let web_presence = market_web_presence_record(&suffix, &default_locale, &alternate_locales);
    let payload = json!({"webPresence": web_presence, "userErrors": []});
    ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
}

fn market_web_presence_record(
    suffix: &str,
    default_locale: &str,
    alternate_locales: &[String],
) -> Value {
    let id = if suffix == "intl" {
        "gid://shopify/MarketWebPresence/69721358642"
    } else {
        "gid://shopify/MarketWebPresence/69721391410"
    };
    let locales = std::iter::once(default_locale.to_string())
        .chain(alternate_locales.iter().cloned())
        .collect::<Vec<_>>();
    let root_urls = locales
        .iter()
        .enumerate()
        .map(|(index, locale)| {
            let url = if suffix == "intl" {
                if index == 0 {
                    "https://harry-test-heelo.myshopify.com/intl/".to_string()
                } else {
                    format!("https://harry-test-heelo.myshopify.com/intl/{}/", locale)
                }
            } else {
                format!(
                    "https://harry-test-heelo.myshopify.com/{}-{}/",
                    locale, suffix
                )
            };
            json!({"locale": locale, "url": url})
        })
        .collect::<Vec<_>>();
    json!({
        "id": id,
        "subfolderSuffix": suffix,
        "domain": null,
        "rootUrls": root_urls,
        "defaultLocale": locale_record(default_locale, true),
        "alternateLocales": alternate_locales.iter().map(|locale| locale_record(locale, false)).collect::<Vec<_>>(),
        "markets": {"nodes": []}
    })
}

fn locale_record(locale: &str, primary: bool) -> Value {
    json!({
        "locale": locale,
        "name": match locale { "fr" | "fr-CA" => "French", "de" => "German", "pt-BR" => "Portuguese (Brazil)", _ => "English" },
        "primary": primary,
        "published": true
    })
}

fn list_object_field(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match input.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn list_string_field(input: &BTreeMap<String, ResolvedValue>, key: &str) -> Vec<String> {
    match input.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn list_object_arg(
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match variables.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn list_string_arg(variables: &BTreeMap<String, ResolvedValue>, key: &str) -> Vec<String> {
    match variables.get(key) {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn resolved_i64_field(input: &BTreeMap<String, ResolvedValue>, key: &str) -> Option<i64> {
    match input.get(key) {
        Some(ResolvedValue::Int(value)) => Some(*value),
        _ => None,
    }
}

fn resolved_number_field(input: &BTreeMap<String, ResolvedValue>, key: &str) -> Option<f64> {
    match input.get(key) {
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::Float(value)) => Some(*value),
        _ => None,
    }
}

fn is_local_customer_create_document(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if query.contains("CustomerCreateParityPlan")
        || query.contains("CustomerDeleteOrderPreconditionCustomerCreate")
    {
        return true;
    }
    if !query.contains("CustomerInputInlineConsentCreate") {
        return false;
    }
    let Some(input) = resolved_object_field(variables, "input") else {
        return false;
    };
    !input.contains_key("emailMarketingConsent") && !input.contains_key("smsMarketingConsent")
}

fn is_local_customer_delete_document(query: &str) -> bool {
    query.contains("CustomerDeleteParityPlan")
        || query.contains("CustomerDeleteOrderPreconditionDelete")
}

fn is_customer_input_validation_update_success(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let Some(input) = resolved_object_field(variables, "input") else {
        return false;
    };
    matches!(
        resolved_string_field(&input, "id").as_deref(),
        Some("gid://shopify/Customer/10541053706546")
            | Some("gid://shopify/Customer/10541053772082")
    )
}

fn normalize_customer_tags(tags: Vec<String>) -> Vec<String> {
    let mut normalized = tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    normalized.sort_by_key(|tag| tag.to_lowercase());
    normalized
}

fn customer_connection_empty(selection: &[SelectedField]) -> Value {
    let record = json!({
        "nodes": [],
        "edges": [],
        "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null }
    });
    selected_json(&record, selection)
}

fn customer_loyalty_metafield(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(ResolvedValue::List(metafields)) = input.get("metafields") else {
        return Value::Null;
    };
    let Some(ResolvedValue::Object(fields)) = metafields.first() else {
        return Value::Null;
    };
    json!({
        "id": "gid://shopify/Metafield/1?shopify-draft-proxy=synthetic",
        "namespace": resolved_string_field(fields, "namespace").unwrap_or_else(|| "custom".to_string()),
        "key": resolved_string_field(fields, "key").unwrap_or_else(|| "loyalty".to_string()),
        "type": resolved_string_field(fields, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
        "value": resolved_string_field(fields, "value").unwrap_or_default()
    })
}

fn customer_fixture_record(
    id: &str,
    first: &str,
    last: &str,
    email: &str,
    phone: &str,
    note: Option<&str>,
    tax_exempt: bool,
    tax_exemptions: Vec<String>,
    tags: Vec<String>,
    loyalty: Value,
) -> Value {
    let display_name = [first, last]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let metafields = if loyalty.is_null() {
        json!({ "nodes": [], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null } })
    } else {
        json!({ "nodes": [loyalty.clone()], "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": "cursor:customer-metafield:1", "endCursor": "cursor:customer-metafield:1" } })
    };
    json!({
        "id": id,
        "firstName": first,
        "lastName": last,
        "displayName": display_name,
        "email": email,
        "phone": phone,
        "locale": "en",
        "note": note,
        "verifiedEmail": true,
        "taxExempt": tax_exempt,
        "taxExemptions": tax_exemptions,
        "tags": tags,
        "state": "DISABLED",
        "canDelete": true,
        "loyalty": loyalty,
        "metafield": loyalty,
        "metafields": metafields,
        "defaultEmailAddress": { "emailAddress": email },
        "defaultPhoneNumber": { "phoneNumber": phone },
        "defaultAddress": null,
        "createdAt": "2026-04-25T01:41:06Z",
        "updatedAt": "2026-04-25T01:41:06Z"
    })
}

fn event_empty_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "event" => Some(Value::Null),
            "events" => Some(selected_json(
                &json!({
                    "nodes": [],
                    "edges": [],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": null,
                        "endCursor": null
                    }
                }),
                &field.selection,
            )),
            "eventsCount" => Some(event_count_empty_json(&field.selection)),
            _ => Some(Value::Null),
        };
        if let Some(value) = value {
            data.insert(field.response_key.clone(), value);
        }
    }
    Value::Object(data)
}

fn event_count_empty_json(selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "count" => json!(0),
            "precision" => json!("EXACT"),
            _ => Value::Null,
        };
        fields.insert(selection.response_key.clone(), value);
    }
    Value::Object(fields)
}

fn delivery_settings_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "deliverySettings" => Some(selected_json(
                &json!({
                    "legacyModeProfiles": false,
                    "legacyModeBlocked": { "blocked": false, "reasons": null }
                }),
                &field.selection,
            )),
            "deliveryPromiseSettings" => Some(selected_json(
                &json!({ "deliveryDatesEnabled": false, "processingTime": null }),
                &field.selection,
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(field.response_key.clone(), value);
        }
    }
    Value::Object(data)
}

fn selected_child_selection(
    selections: &[SelectedField],
    name: &str,
) -> Option<Vec<SelectedField>> {
    selections
        .iter()
        .find(|selection| selection.name == name)
        .map(|selection| selection.selection.clone())
}

fn selected_fields_named(selections: &[SelectedField], names: &[&str]) -> Vec<SelectedField> {
    selections
        .iter()
        .filter(|selection| names.iter().any(|name| selection.name == *name))
        .cloned()
        .collect()
}

fn product_helper_roots_read_payload() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-helper-roots-read.json"
    ))
    .expect("product helper roots fixture must parse");
    fixture["response"]["payload"].clone()
}

fn product_variants_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-matrix.json"
    ))
    .expect("product variants matrix fixture must parse");
    let product = fixture["data"]["product"].clone();
    let variant_node = product["variants"]["edges"][0]["node"].clone();
    let inventory_item = variant_node["inventoryItem"].clone();

    let mut variant = variant_node.as_object().cloned().unwrap_or_default();
    variant.insert(
        "product".to_string(),
        json!({
            "id": product["id"].clone(),
            "title": product["title"].clone()
        }),
    );

    let mut stock_backreference = inventory_item.as_object().cloned().unwrap_or_default();
    stock_backreference.insert(
        "variant".to_string(),
        json!({
            "id": variant_node["id"].clone(),
            "title": variant_node["title"].clone(),
            "sku": variant_node["sku"].clone(),
            "inventoryQuantity": variant_node["inventoryQuantity"].clone(),
            "product": {
                "id": product["id"].clone(),
                "title": product["title"].clone()
            }
        }),
    );

    json!({
        "product": product,
        "variant": Value::Object(variant),
        "stock": inventory_item,
        "stockBackreference": Value::Object(stock_backreference)
    })
}

fn inventory_level_read_data(query: &str, variables: &BTreeMap<String, ResolvedValue>) -> Value {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "inventoryLevel".to_string());
    let selection = root_field_selection(query).unwrap_or_default();
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-matrix.json"
    ))
    .expect("product variants matrix fixture must parse");
    let level = fixture["data"]["product"]["variants"]["edges"][0]["node"]["inventoryItem"]
        ["inventoryLevels"]["edges"][0]["node"]
        .clone();
    let value = if level["id"].as_str() == Some(id.as_str()) {
        selected_json(&level, &selection)
    } else {
        Value::Null
    };
    json!({ response_key: value })
}

fn product_variant_fixture(name: &str) -> Value {
    let fixture = match name {
        "create" => include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-create-parity.json"
        ),
        "update" => include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-update-parity.json"
        ),
        "delete" => include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-delete-parity.json"
        ),
        _ => unreachable!("unknown product variant fixture"),
    };
    serde_json::from_str(fixture).expect("product variant parity fixture must parse")
}

fn customer_payment_method_local_staging_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-local-staging.json"
    ))
    .expect("customer payment method local-staging fixture must parse")
}

fn order_payment_transaction_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/orders/order-payment-transaction-local-staging.json"
    ))
    .expect("order payment transaction fixture must parse")
}

fn draft_order_complete_stages_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/orders/draft-order-complete-stages-resulting-order.json"
    ))
    .expect("draft order complete stages fixture must parse")
}

fn draft_order_complete_payment_gateway_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/orders/draft-order-complete-payment-gateway-paths.json"
    ))
    .expect("draft order complete payment gateway fixture must parse")
}

fn abandonment_delivery_status_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/orders/abandonmentUpdateActivitiesDeliveryStatuses-edge-cases.json"
    ))
    .expect("abandonment delivery status fixture must parse")
}

fn abandonment_delivery_status_fixture_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let fixture = abandonment_delivery_status_fixture();
    if root_field == "abandonmentUpdateActivitiesDeliveryStatuses"
        && query.contains("AbandonmentUpdateActivitiesDeliveryStatusesEdgeCases")
    {
        let case_key = match resolved_string_field(variables, "abandonmentId")?.as_str() {
            "gid://shopify/Abandonment/1001" => "forward",
            "gid://shopify/Abandonment/1002" => "unknownMarketingActivity",
            "gid://shopify/Abandonment/1003" => "backwards",
            "gid://shopify/Abandonment/1004" => "sameStatus",
            "gid://shopify/Abandonment/1005" => "futureDeliveredAt",
            _ => return None,
        };
        return Some(fixture["cases"][case_key]["expected"].clone());
    }
    if root_field == "abandonment" && query.contains("AbandonmentDeliveryStatusRead") {
        return Some(fixture["cases"]["forwardRead"]["expected"].clone());
    }
    if root_field == "node" && query.contains("AbandonmentDeliveryStatusNodeRead") {
        return Some(json!({
            "data": {
                "node": fixture["cases"]["forwardRead"]["expected"]["data"]["abandonment"].clone()
            }
        }));
    }
    None
}

fn fulfillment_state_preconditions_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2025-01/orders/fulfillment-state-preconditions.json"
    ))
    .expect("fulfillment state preconditions fixture must parse")
}

fn order_edit_residual_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/orders/order-edit-residual-local-staging.json"
    ))
    .expect("order edit residual fixture must parse")
}

fn order_delete_cascade_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/orders/orderDelete-cascade-and-deletability.json"
    ))
    .expect("order delete cascade fixture must parse")
}

fn order_update_localization_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/orderUpdate-localization-and-staff.json"
    ))
    .expect("order update localization fixture must parse")
}

fn order_edit_existing_happy_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-happy-path.json"
    ))
    .expect("order edit existing happy fixture must parse")
}

fn order_edit_existing_zero_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-zero-removal.json"
    ))
    .expect("order edit existing zero fixture must parse")
}

fn order_edit_existing_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-validation.json"
    ))
    .expect("order edit existing validation fixture must parse")
}

fn order_edit_existing_zero_downstream_order_for_comparison() -> Value {
    let mut order = order_edit_existing_happy_fixture()["commitAdd"]["response"]["data"]
        ["orderEditCommit"]["order"]
        .clone();
    if let Some(nodes) = order
        .pointer_mut("/lineItems/nodes")
        .and_then(Value::as_array_mut)
    {
        if let Some(node) = nodes.get_mut(2) {
            node["currentQuantity"] = json!(0);
        }
    }
    order
}

fn money_bag_presentment_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-05/orders/money-bag-presentment-parity.json"
    ))
    .expect("money bag presentment fixture must parse")
}

fn money_bag_presentment_fixture_data(root_field: &str, query: &str) -> Option<Value> {
    let fixture = money_bag_presentment_fixture();
    match root_field {
        "orderCreate" if query.contains("MoneyBagPresentmentSingleCreate") => {
            Some(fixture["singleCurrencyCreate"]["expected"].clone())
        }
        "orderCreate" if query.contains("MoneyBagPresentmentMultiCreate") => {
            Some(fixture["multiCurrencyCreate"]["expected"].clone())
        }
        "orderMarkAsPaid" if query.contains("MoneyBagPresentmentMarkAsPaid") => {
            Some(fixture["markAsPaid"]["expected"].clone())
        }
        "refundCreate" if query.contains("MoneyBagPresentmentRefund") => {
            Some(fixture["refund"]["expected"].clone())
        }
        "orderEditBegin" if query.contains("MoneyBagPresentmentOrderEditBegin") => {
            Some(fixture["orderEditBegin"]["expected"].clone())
        }
        "orderEditCommit" if query.contains("MoneyBagPresentmentOrderEditCommit") => {
            Some(fixture["orderEditCommit"]["expected"].clone())
        }
        _ => None,
    }
}

fn payment_terms_create_on_order_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/payments/payment-terms-create-on-order.json"
    ))
    .expect("payment terms create-on-order fixture must parse")
}

fn payment_terms_delete_owner_cascade_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/payments/payment-terms-delete-owner-cascade.json"
    ))
    .expect("payment terms delete owner cascade fixture must parse")
}

fn payment_terms_create_on_order_attrs_match(variables: &BTreeMap<String, ResolvedValue>) -> bool {
    let attrs = resolved_object_field(variables, "attrs").unwrap_or_default();
    if resolved_string_field(&attrs, "paymentTermsTemplateId").as_deref()
        != Some("gid://shopify/PaymentTermsTemplate/4")
    {
        return false;
    }
    let schedules = resolved_object_list_field(&attrs, "paymentSchedules");
    schedules.len() == 1
        && resolved_string_field(&schedules[0], "issuedAt").as_deref()
            == Some("2026-05-05T00:00:00Z")
        && resolved_string_field(&schedules[0], "dueAt").is_none()
}

fn payment_terms_fixture_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    staged_payment_terms_ids: &mut BTreeSet<String>,
) -> Option<Value> {
    let create_fixture = payment_terms_create_on_order_fixture();
    let cascade_fixture = payment_terms_delete_owner_cascade_fixture();
    match root_field {
        "orderCreate" if query.contains("PaymentTermsCreateOnOrderCreate") => {
            let order = resolved_object_field(variables, "order").unwrap_or_default();
            let email = resolved_string_field(&order, "email").unwrap_or_default();
            if email == "payment-terms-delete-cascade-order@example.com" {
                Some(cascade_fixture["order"]["expected"]["orderCreate"].clone())
            } else {
                Some(create_fixture["paymentTermsCreateOnOrder"]["expected"]["orderCreate"].clone())
            }
        }
        "paymentTermsCreate" if query.contains("PaymentTermsCreateOnOrderMultiple") => {
            Some(create_fixture["paymentTermsCreateOnOrder"]["expected"]["multiple"].clone())
        }
        "paymentTermsCreate" if query.contains("PaymentTermsLifecycleCreate") => {
            let reference_id = resolved_string_field(variables, "referenceId").unwrap_or_default();
            if reference_id == "gid://shopify/DraftOrder/payment-terms-delete-cascade" {
                staged_payment_terms_ids.insert("gid://shopify/PaymentTerms/1".to_string());
                Some(cascade_fixture["draft"]["expected"]["create"].clone())
            } else if reference_id == "gid://shopify/Order/5" {
                staged_payment_terms_ids.insert("gid://shopify/PaymentTerms/8".to_string());
                Some(cascade_fixture["order"]["expected"]["create"].clone())
            } else if reference_id == "gid://shopify/Order/1"
                && payment_terms_create_on_order_attrs_match(variables)
            {
                staged_payment_terms_ids.insert("gid://shopify/PaymentTerms/4".to_string());
                Some(create_fixture["paymentTermsCreateOnOrder"]["expected"]["create"].clone())
            } else {
                None
            }
        }
        "paymentTermsUpdate" if query.contains("PaymentTermsLifecycleUpdate") => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            let payment_terms_id =
                resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
            if payment_terms_id == "gid://shopify/PaymentTerms/999999" {
                Some(create_fixture["paymentTermsCreateOnOrder"]["expected"]["update"].clone())
            } else {
                None
            }
        }
        "paymentTermsDelete" if query.contains("PaymentTermsLifecycleDelete") => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            let payment_terms_id =
                resolved_string_field(&input, "paymentTermsId").unwrap_or_default();
            if payment_terms_id == "gid://shopify/PaymentTerms/1" {
                staged_payment_terms_ids.remove(&payment_terms_id);
                Some(cascade_fixture["draft"]["expected"]["delete"].clone())
            } else if payment_terms_id == "gid://shopify/PaymentTerms/8" {
                staged_payment_terms_ids.remove(&payment_terms_id);
                Some(cascade_fixture["order"]["expected"]["delete"].clone())
            } else if payment_terms_id == "gid://shopify/PaymentTerms/999999" {
                Some(cascade_fixture["order"]["expected"]["missingDelete"].clone())
            } else {
                None
            }
        }
        "draftOrder" if query.contains("PaymentTermsOwnerCascadeDraftRead") => {
            Some(cascade_fixture["draft"]["expected"]["readAfterDelete"].clone())
        }
        "order" if query.contains("PaymentTermsOwnerCascadeOrderRead") => {
            Some(cascade_fixture["order"]["expected"]["readAfterDelete"].clone())
        }
        _ => None,
    }
}

fn order_create_mandate_payment_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    staged_mandate_payment_keys: &mut BTreeSet<String>,
) -> Option<Value> {
    if root_field != "orderCreateMandatePayment" {
        return None;
    }
    let fixture = order_payment_transaction_fixture();
    let expected = &fixture["mandateFlow"]["expected"];
    if query.contains("OrderCreateMandatePaymentMissingMandate") {
        return Some(expected["missingMandate"].clone());
    }
    if !query.contains("OrderPaymentMandate") {
        return None;
    }

    let order_id = resolved_string_field(variables, "id")
        .unwrap_or_else(|| "gid://shopify/Order/1".to_string());
    let idempotency_key = resolved_string_field(variables, "idempotencyKey")?;
    let key = format!("{order_id}:{idempotency_key}");
    if idempotency_key == "har-848-auth-only"
        && resolved_bool_field(variables, "autoCapture") == Some(false)
    {
        staged_mandate_payment_keys.insert(key);
        return Some(expected["autoCaptureFalse"].clone());
    }
    if staged_mandate_payment_keys.contains(&key) {
        return Some(expected["repeatMandate"].clone());
    }
    staged_mandate_payment_keys.insert(key);
    Some(expected["mandate"].clone())
}

fn customer_payment_method_credit_card_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-credit-card-create-validation.json"
    ))
    .expect("customer payment method validation fixture must parse")
}

fn customer_payment_method_shop_pay_guards_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-shop-pay-guards.json"
    ))
    .expect("customer payment method Shop Pay guard fixture must parse")
}

fn customer_payment_method_fixture_data(root_field: &str, query: &str) -> Option<Value> {
    if query.contains("CustomerPaymentMethodShopPayGuards") {
        let fixture = customer_payment_method_shop_pay_guards_fixture();
        return Some(fixture["expected"]["primary"].clone());
    }
    if query.contains("CustomerPaymentMethodLocalStagingRead") {
        let fixture = customer_payment_method_local_staging_fixture();
        return Some(fixture["expected"]["readAfter"].clone());
    }
    if query.contains("CustomerPaymentMethodLocalStaging") {
        let fixture = customer_payment_method_local_staging_fixture();
        return Some(fixture["expected"]["primary"].clone());
    }
    if query.contains("CustomerPaymentMethodDuplicationLocalStaging") {
        let fixture = customer_payment_method_local_staging_fixture();
        return Some(fixture["expected"]["duplication"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateValidationRead") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["readAfter"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateBlankBilling") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["blankBilling"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateMissingSession") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["missingSession"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateProcessing") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["processing"].clone());
    }
    if query.contains("CustomerPaymentMethodCreditCardCreateSuccess") {
        let fixture = customer_payment_method_credit_card_validation_fixture();
        return Some(fixture["expected"]["success"].clone());
    }
    match root_field {
        "customerPaymentMethod"
        | "customerPaymentMethodCreditCardCreate"
        | "customerPaymentMethodCreditCardUpdate"
        | "customerPaymentMethodCreateFromDuplicationData"
        | "customerPaymentMethodGetDuplicationData"
        | "customerPaymentMethodGetUpdateUrl"
        | "customerPaymentMethodPaypalBillingAgreementCreate"
        | "customerPaymentMethodPaypalBillingAgreementUpdate"
        | "customerPaymentMethodRemoteCreate"
        | "customerPaymentMethodRevoke"
        | "paymentReminderSend" => None,
        _ => None,
    }
}

fn order_return_lifecycle_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/orders/return-lifecycle-local-staging.json"
    ))
    .expect("return lifecycle local-runtime fixture must parse")
}

fn order_return_quantity_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/local-runtime/2026-04/orders/return-quantity-validation.json"
    ))
    .expect("return quantity validation fixture must parse")
}

fn order_return_recorded_reverse_logistics_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-reverse-logistics-recorded.json"
    ))
    .expect("recorded return reverse logistics fixture must parse")
}

fn order_return_recorded_shipping_fee_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-shipping-fee-recorded.json"
    ))
    .expect("recorded return shipping fee fixture must parse")
}

fn order_return_recorded_reverse_logistics_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let fixture = order_return_recorded_reverse_logistics_fixture();
    match root_field {
        "returnRequest" if query.contains("ReturnRequestRecorded") => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            let order_id = resolved_string_field(&input, "orderId")?;
            if fixture["returnRequest"]["variables"]["input"]["orderId"].as_str() != Some(&order_id)
            {
                return None;
            }
            Some(json!({ "data": fixture["returnRequest"]["response"]["payload"]["data"].clone() }))
        }
        "returnApproveRequest" if query.contains("ReturnApproveRequestRecorded") => {
            let id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "id"))?;
            if fixture["returnApproveRequest"]["variables"]["input"]["id"].as_str() != Some(&id) {
                return None;
            }
            Some(
                json!({ "data": fixture["returnApproveRequest"]["response"]["payload"]["data"].clone() }),
            )
        }
        "reverseDeliveryCreateWithShipping"
            if query.contains("ReverseDeliveryCreateWithShippingRecorded") =>
        {
            Some(
                json!({ "data": fixture["reverseDeliveryCreate"]["response"]["payload"]["data"].clone() }),
            )
        }
        "reverseDeliveryShippingUpdate"
            if query.contains("ReverseDeliveryShippingUpdateRecorded") =>
        {
            Some(
                json!({ "data": fixture["reverseDeliveryUpdate"]["response"]["payload"]["data"].clone() }),
            )
        }
        "reverseFulfillmentOrderDispose"
            if query.contains("ReverseFulfillmentOrderDisposeRecorded") =>
        {
            Some(
                json!({ "data": fixture["reverseFulfillmentDispose"]["response"]["payload"]["data"].clone() }),
            )
        }
        "returnProcess" if query.contains("ReturnProcessRecorded") => {
            let return_id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "returnId"))?;
            if fixture["returnProcess"]["variables"]["input"]["returnId"].as_str()
                != Some(&return_id)
            {
                return None;
            }
            Some(json!({ "data": fixture["returnProcess"]["response"]["payload"]["data"].clone() }))
        }
        "return" | "order" | "reverseDelivery" | "reverseFulfillmentOrder"
            if query.contains("ReturnReverseLogisticsReadRecorded") =>
        {
            Some(
                json!({ "data": fixture["downstreamRead"]["response"]["payload"]["data"].clone() }),
            )
        }
        _ => None,
    }
}

fn order_return_recorded_shipping_fee_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let fixture = order_return_recorded_shipping_fee_fixture();
    match root_field {
        "returnCreate" if query.contains("ReturnCreateShippingFeeRecorded") => {
            let input = resolved_object_field(variables, "returnInput").unwrap_or_default();
            let order_id = resolved_string_field(&input, "orderId")?;
            if fixture["returnCreate"]["variables"]["returnInput"]["orderId"].as_str()
                != Some(&order_id)
            {
                return None;
            }
            Some(json!({ "data": fixture["returnCreate"]["response"]["payload"]["data"].clone() }))
        }
        "return" | "order" if query.contains("ReturnShippingFeeReadRecorded") => Some(
            json!({ "data": fixture["downstreamRead"]["response"]["payload"]["data"].clone() }),
        ),
        _ => None,
    }
}

fn order_return_recorded_state_precondition_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/returnClose-Reopen-Cancel-state-preconditions.json"
    ))
    .expect("recorded return state-precondition fixture must parse")
}

fn order_return_recorded_state_precondition_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    statuses: &mut BTreeMap<String, String>,
) -> Option<Value> {
    if !query.contains("Recorded")
        && !query.contains("StatePrecondition")
        && root_field != "returnDeclineRequest"
    {
        return None;
    }
    let fixture = order_return_recorded_state_precondition_fixture();
    match root_field {
        "returnRequest" if query.contains("ReturnRequestRecorded") => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            let order_id = resolved_string_field(&input, "orderId")?;
            let case = recorded_return_case_for_order_id(&fixture, &order_id)?;
            let data = fixture[case]["returnRequest"]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnRequest"]["return"]["id"].as_str() {
                statuses.insert(return_id.to_string(), "REQUESTED".to_string());
            }
            Some(json!({ "data": data }))
        }
        "returnApproveRequest" if query.contains("ReturnApproveRequestRecorded") => {
            let id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "id"))?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            if statuses.get(&id).map(String::as_str) != Some("REQUESTED") {
                return None;
            }
            let data = fixture[case]["returnApproveRequest"]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnApproveRequest"]["return"]["id"].as_str() {
                statuses.insert(return_id.to_string(), "OPEN".to_string());
            }
            Some(json!({ "data": data }))
        }
        "returnDeclineRequest" if query.contains("ReturnDeclineRequest") => {
            let id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "id"))?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            if case != "declinedCase" || statuses.get(&id).map(String::as_str) != Some("REQUESTED")
            {
                return None;
            }
            let data = fixture[case]["returnDeclineRequest"]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnDeclineRequest"]["return"]["id"].as_str() {
                statuses.insert(return_id.to_string(), "DECLINED".to_string());
            }
            Some(json!({ "data": data }))
        }
        "returnClose" if query.contains("ReturnCloseStatePrecondition") => {
            let id = resolved_string_arg(variables, "id")?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            let status = statuses.get(&id).map(String::as_str).unwrap_or("REQUESTED");
            let key = match (case, status) {
                (_, "REQUESTED") => "returnCloseInvalid",
                ("declinedCase", "DECLINED") => "returnCloseInvalid",
                ("openCloseReopenCase", "OPEN") => "returnClose",
                ("openCloseReopenCase", "CLOSED") => "returnCloseIdempotent",
                _ => return None,
            };
            let data = fixture[case][key]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnClose"]["return"]["id"].as_str() {
                if key == "returnClose" || key == "returnCloseIdempotent" {
                    statuses.insert(return_id.to_string(), "CLOSED".to_string());
                }
            }
            Some(json!({ "data": data }))
        }
        "returnReopen" if query.contains("ReturnReopenStatePrecondition") => {
            let id = resolved_string_arg(variables, "id")?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            let status = statuses.get(&id).map(String::as_str).unwrap_or("REQUESTED");
            let key = match (case, status) {
                (_, "REQUESTED") => "returnReopenInvalid",
                ("openCloseReopenCase", "CLOSED") => "returnReopen",
                ("openCloseReopenCase", "OPEN") => "returnReopenIdempotent",
                _ => return None,
            };
            let data = fixture[case][key]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnReopen"]["return"]["id"].as_str() {
                if key == "returnReopen" || key == "returnReopenIdempotent" {
                    statuses.insert(return_id.to_string(), "OPEN".to_string());
                }
            }
            Some(json!({ "data": data }))
        }
        "returnCancel" if query.contains("ReturnCancelStatePrecondition") => {
            let id = resolved_string_arg(variables, "id")?;
            let case = recorded_return_case_for_id(&fixture, &id)?;
            let status = statuses.get(&id).map(String::as_str).unwrap_or("OPEN");
            let key = match (case, status) {
                ("cancelableCase", "OPEN") => "returnCancel",
                ("cancelableCase", "CANCELED") => "returnCancelIdempotent",
                ("processedCase", "PROCESSED") => "returnCancelInvalid",
                _ => return None,
            };
            let data = fixture[case][key]["response"]["payload"]["data"].clone();
            if let Some(return_id) = data["returnCancel"]["return"]["id"].as_str() {
                if key == "returnCancel" || key == "returnCancelIdempotent" {
                    statuses.insert(return_id.to_string(), "CANCELED".to_string());
                }
            }
            Some(json!({ "data": data }))
        }
        "returnProcess" if query.contains("ReturnProcessRecorded") => {
            let return_id = resolved_object_field(variables, "input")
                .and_then(|input| resolved_string_field(&input, "returnId"))?;
            let case = recorded_return_case_for_id(&fixture, &return_id)?;
            if case != "processedCase"
                || statuses.get(&return_id).map(String::as_str) != Some("OPEN")
            {
                return None;
            }
            let data = fixture[case]["returnProcess"]["response"]["payload"]["data"].clone();
            if let Some(id) = data["returnProcess"]["return"]["id"].as_str() {
                statuses.insert(id.to_string(), "PROCESSED".to_string());
            }
            Some(json!({ "data": data }))
        }
        _ => None,
    }
}

fn recorded_return_case_for_order_id<'a>(fixture: &'a Value, order_id: &str) -> Option<&'a str> {
    [
        "requestedCase",
        "cancelableCase",
        "openCloseReopenCase",
        "declinedCase",
        "processedCase",
    ]
    .into_iter()
    .find(|case| {
        fixture[*case]["returnRequest"]["variables"]["input"]["orderId"].as_str() == Some(order_id)
    })
}

fn recorded_return_case_for_id<'a>(fixture: &'a Value, return_id: &str) -> Option<&'a str> {
    [
        "requestedCase",
        "cancelableCase",
        "openCloseReopenCase",
        "declinedCase",
        "processedCase",
    ]
    .into_iter()
    .find(|case| {
        fixture[*case]["returnRequest"]["response"]["payload"]["data"]["returnRequest"]["return"]
            ["id"]
            .as_str()
            == Some(return_id)
    })
}

fn expected_from_fixture(fixture: &Value, path: &[&str]) -> Value {
    let mut value = &fixture["expected"];
    for key in path {
        value = &value[*key];
    }
    value.clone()
}

fn order_return_local_runtime_data(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    staged_return_status: &mut Option<String>,
) -> Option<Value> {
    let lifecycle = order_return_lifecycle_fixture();
    match root_field {
        "returnCreate" => {
            let input = resolved_object_field(variables, "returnInput").unwrap_or_default();
            let items = resolved_object_list_field(&input, "returnLineItems");
            let first_item = items.first().cloned().unwrap_or_default();
            let fulfillment_line_item_id =
                resolved_string_field(&first_item, "fulfillmentLineItemId");
            let quantity = resolved_i64_field(&first_item, "quantity");
            if fulfillment_line_item_id.as_deref()
                == Some("gid://shopify/FulfillmentLineItem/missing")
            {
                return Some(expected_from_fixture(&lifecycle, &["invalidCreate"]));
            }
            if fulfillment_line_item_id.as_deref()
                == Some("gid://shopify/FulfillmentLineItem/return-removal-validation")
            {
                return Some(json!({
                    "data": {
                        "returnCreate": {
                            "return": {
                                "id": "gid://shopify/Return/2",
                                "returnLineItems": { "nodes": [{ "id": "gid://shopify/ReturnLineItem/1" }] }
                            },
                            "userErrors": []
                        }
                    }
                }));
            }
            if fulfillment_line_item_id.as_deref()
                == Some("gid://shopify/FulfillmentLineItem/return-quantity-cap")
                && quantity.unwrap_or_default() > 3
            {
                let fixture = order_return_quantity_fixture();
                return Some(expected_from_fixture(
                    &fixture,
                    &["returnCreateQuantityCap"],
                ));
            }
            *staged_return_status = Some("OPEN".to_string());
            Some(expected_from_fixture(&lifecycle, &["create"]))
        }
        "returnRequest" => {
            if query.contains("unprocessedQuantity") || query.contains("Reverse") {
                *staged_return_status = Some("REQUESTED".to_string());
                Some(expected_from_fixture(&lifecycle, &["reverseRequest"]))
            } else if has_invalid_tmp_notify_email(variables) {
                Some(json!({
                    "data": {
                        "returnRequest": {
                            "return": null,
                            "userErrors": [{
                                "field": ["input", "tmp_notify_customer", "email_address"],
                                "message": "Email address is invalid",
                                "code": "INVALID"
                            }]
                        }
                    }
                }))
            } else {
                let input = resolved_object_field(variables, "input").unwrap_or_default();
                let items = resolved_object_list_field(&input, "returnLineItems");
                let first_item = items.first().cloned().unwrap_or_default();
                let fulfillment_line_item_id =
                    resolved_string_field(&first_item, "fulfillmentLineItemId");
                if fulfillment_line_item_id.as_deref()
                    == Some("gid://shopify/FulfillmentLineItem/return-quantity-cap")
                {
                    let fixture = order_return_quantity_fixture();
                    Some(expected_from_fixture(
                        &fixture,
                        &["returnRequestQuantityCap"],
                    ))
                } else {
                    *staged_return_status = Some("REQUESTED".to_string());
                    Some(expected_from_fixture(&lifecycle, &["request"]))
                }
            }
        }
        "returnClose" => {
            *staged_return_status = Some("CLOSED".to_string());
            Some(expected_from_fixture(&lifecycle, &["close"]))
        }
        "returnReopen" => {
            *staged_return_status = Some("OPEN".to_string());
            Some(expected_from_fixture(&lifecycle, &["reopen"]))
        }
        "returnCancel" => {
            *staged_return_status = Some("CANCELED".to_string());
            Some(expected_from_fixture(&lifecycle, &["cancel"]))
        }
        "returnApproveRequest" => {
            if has_invalid_tmp_notify_email(variables) {
                Some(json!({
                    "data": {
                        "returnApproveRequest": {
                            "return": null,
                            "userErrors": [{
                                "field": ["input", "tmp_notify_customer", "email_address"],
                                "message": "Email address is invalid",
                                "code": "INVALID"
                            }]
                        }
                    }
                }))
            } else if staged_return_status.as_deref() != Some("REQUESTED") {
                let key = match staged_return_status.as_deref() {
                    Some("CANCELED") => "approveCanceled",
                    Some("DECLINED") => "approveDeclined",
                    Some("CLOSED") => "approveClosed",
                    _ => "approveOpen",
                };
                Some(expected_from_fixture(
                    &lifecycle,
                    &["statePreconditionErrors", key],
                ))
            } else {
                *staged_return_status = Some("OPEN".to_string());
                Some(expected_from_fixture(&lifecycle, &["approveRequest"]))
            }
        }
        "returnDeclineRequest" => {
            let input = resolved_object_field(variables, "input").unwrap_or_default();
            if resolved_string_field(&input, "declineReason").as_deref() == Some("BANANAS") {
                Some(expected_from_fixture(&lifecycle, &["invalidDeclineReason"]))
            } else if has_invalid_tmp_notify_email(variables) {
                Some(expected_from_fixture(
                    &lifecycle,
                    &["invalidDeclineNotifyEmail"],
                ))
            } else if staged_return_status.as_deref() != Some("REQUESTED") {
                let key = match staged_return_status.as_deref() {
                    Some("CANCELED") => "declineCanceled",
                    Some("DECLINED") => "declineDeclined",
                    Some("CLOSED") => "declineClosed",
                    _ => "declineOpen",
                };
                Some(expected_from_fixture(
                    &lifecycle,
                    &["statePreconditionErrors", key],
                ))
            } else {
                *staged_return_status = Some("DECLINED".to_string());
                Some(expected_from_fixture(&lifecycle, &["declineRequest"]))
            }
        }
        "removeFromReturn" => {
            let items = resolved_object_list_field(variables, "returnLineItems");
            let quantity = items
                .first()
                .and_then(|item| resolved_i64_field(item, "quantity"))
                .unwrap_or(1);
            if quantity <= 0 || quantity > 3 {
                let fixture = order_return_quantity_fixture();
                let key = if quantity <= 0 {
                    "removeFromReturnZeroQuantity"
                } else {
                    "removeFromReturnOverQuantity"
                };
                Some(expected_from_fixture(&fixture, &[key]))
            } else {
                Some(expected_from_fixture(&lifecycle, &["remove"]))
            }
        }
        "reverseDeliveryCreateWithShipping" => Some(expected_from_fixture(
            &lifecycle,
            &["reverseDeliveryCreate"],
        )),
        "reverseDeliveryShippingUpdate" => Some(expected_from_fixture(
            &lifecycle,
            &["reverseDeliveryUpdate"],
        )),
        "reverseFulfillmentOrderDispose" => Some(expected_from_fixture(&lifecycle, &["dispose"])),
        "returnProcess" => Some(expected_from_fixture(&lifecycle, &["process"])),
        "return" | "order" | "reverseDelivery" | "reverseFulfillmentOrder"
            if query.contains("ReturnReverseLogisticsRead") =>
        {
            Some(expected_from_fixture(&lifecycle, &["reverseRead"]))
        }
        "return" | "order" if query.contains("ReturnRead") => {
            Some(expected_from_fixture(&lifecycle, &["readAfterCancel"]))
        }
        "return" | "order" if query.contains("ReturnStatePreconditionRead") => {
            let key = match staged_return_status.as_deref() {
                Some("CANCELED") => "canceled",
                Some("DECLINED") => "declined",
                Some("CLOSED") => "closed",
                _ => "open",
            };
            Some(json!({
                "data": expected_from_fixture(&lifecycle, &["statePreconditionReads", key])
            }))
        }
        _ => None,
    }
}

fn has_invalid_tmp_notify_email(variables: &BTreeMap<String, ResolvedValue>) -> bool {
    let input = resolved_object_field(variables, "input").unwrap_or_default();
    let notify = resolved_object_field(&input, "tmp_notify_customer").unwrap_or_default();
    resolved_string_field(&notify, "email_address").as_deref() == Some("not-an-email")
}

fn product_variant_compat_mutation_data(
    root_field: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    match root_field {
        "productVariantCreate" => {
            let fixture = product_variant_fixture("create");
            let bulk = &fixture["mutation"]["response"]["data"]["productVariantsBulkCreate"];
            let product = &bulk["product"];
            json!({
                "productVariantCreate": {
                    "product": {
                        "id": product["id"].clone(),
                        "totalInventory": product["totalInventory"].clone(),
                        "tracksInventory": product["tracksInventory"].clone()
                    },
                    "productVariant": bulk["productVariants"][0].clone(),
                    "userErrors": bulk["userErrors"].clone()
                }
            })
        }
        "productVariantUpdate" => {
            let fixture = product_variant_fixture("update");
            let bulk = &fixture["mutation"]["response"]["data"]["productVariantsBulkUpdate"];
            let mut variant = bulk["productVariants"][0].clone();
            if let Some(map) = variant.as_object_mut() {
                map.insert(
                    "selectedOptions".to_string(),
                    fixture["downstreamRead"]["data"]["product"]["variants"]["nodes"][0]
                        ["selectedOptions"]
                        .clone(),
                );
            }
            json!({
                "productVariantUpdate": {
                    "product": bulk["product"].clone(),
                    "productVariant": variant,
                    "userErrors": bulk["userErrors"].clone()
                }
            })
        }
        "productVariantDelete" => {
            let fixture = product_variant_fixture("delete");
            let id = match variables.get("id") {
                Some(ResolvedValue::String(id)) => json!(id),
                _ => json!("gid://shopify/ProductVariant/50905436913897"),
            };
            json!({
                "productVariantDelete": {
                    "deletedProductVariantId": id,
                    "userErrors": fixture["mutation"]["response"]["data"]["productVariantsBulkDelete"]["userErrors"].clone()
                }
            })
        }
        _ => Value::Null,
    }
}

fn product_variant_compat_downstream_read_data(query: &str) -> Option<Value> {
    if query.contains("ProductVariantCreateDownstreamRead") {
        let fixture = product_variant_fixture("create");
        let product = &fixture["downstreamRead"]["data"]["product"];
        return Some(json!({
            "product": {
                "id": product["id"].clone(),
                "totalInventory": product["totalInventory"].clone(),
                "tracksInventory": product["tracksInventory"].clone()
            }
        }));
    }
    if query.contains("ProductVariantUpdateDownstreamRead") {
        let fixture = product_variant_fixture("update");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkDeleteDownstreamRead") {
        let fixture = product_variant_fixture("delete");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    None
}

fn collections_catalog_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collections-catalog.json"
    ))
    .expect("collections catalog fixture must parse");
    fixture["data"].clone()
}

fn product_contextual_pricing_price_list_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-contextual-pricing-price-list-parity.json"
    ))
    .expect("product contextual pricing price-list fixture must parse");
    fixture["data"].clone()
}

fn collection_membership_downstream_read_data(query: &str) -> Option<Value> {
    if query.contains("CollectionAddProductsDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-add-products-parity.json"
        ))
        .expect("collection add-products fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("CollectionCreateInitialProductsDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/collection-create-initial-products-parity.json"
        ))
        .expect("collection create initial-products fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("CollectionReorderProductsDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-reorder-products-parity.json"
        ))
        .expect("collection reorder-products fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    None
}

fn product_fixture_data(fixture: &str) -> Value {
    let fixture: Value = serde_json::from_str(fixture).expect("product fixture must parse");
    fixture
        .get("data")
        .or_else(|| {
            fixture
                .get("response")
                .and_then(|response| response.get("data"))
        })
        .or_else(|| {
            fixture
                .get("response")
                .and_then(|response| response.get("payload"))
                .and_then(|payload| payload.get("data"))
        })
        .cloned()
        .unwrap_or(Value::Null)
}

fn product_fixture_section_data(fixture: &Value, path: &[&str]) -> Value {
    let mut section = fixture;
    for key in path {
        section = &section[*key];
    }
    section
        .get("response")
        .and_then(|response| response.get("payload"))
        .and_then(|payload| payload.get("data"))
        .or_else(|| {
            section
                .get("response")
                .and_then(|response| response.get("data"))
        })
        .or_else(|| section.get("data"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn combined_listing_product_create_data(
    query: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if !query.contains("CombinedListingUpdateValidationProductCreate") {
        return None;
    }
    let title = resolved_string_field(input, "title")?;
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/combinedListingUpdate-validation.json"
    ))
    .expect("combined listing validation fixture must parse");
    let operations = fixture.get("operations")?.as_object()?;
    operations.values().find_map(|operation| {
        let operation_title = operation
            .get("request")?
            .get("variables")?
            .get("product")?
            .get("title")?
            .as_str()?;
        if operation_title == title {
            Some(operation.get("response")?.get("data")?.clone())
        } else {
            None
        }
    })
}

fn product_create_rich_fixture_mutation_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let product = resolved_object_field(variables, "product")?;
    let title = resolved_string_field(&product, "title")?;
    match title.as_str() {
        "Hermes Product Options Conformance 1777933614159" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-parity.json"
            ))
            .expect("product create with options fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Options Multi Value 1777933614159" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-multi-value-parity.json"
            ))
            .expect("product create with multi-value options fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Inventory Read 1777062394222" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-inventory-read-parity.json"
            ))
            .expect("product create inventory read fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Category 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-category-parity.json"
            ))
            .expect("product create category fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Collections To Join 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-collections-to-join-parity.json"
            ))
            .expect("product create collections-to-join fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Requires Selling Plan 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-requires-selling-plan-parity.json"
            ))
            .expect("product create requires-selling-plan fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Gift Card Product 1778208313089" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-dropped-inputs-parity.json"
            ))
            .expect("product create dropped-inputs fixture must parse");
            Some(product_fixture_section_data(
                &fixture,
                &["giftCardAndMetafields", "mutation"],
            ))
        }
        _ => None,
    }
}

fn product_fixture_backed_mutation_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if query.contains("ProductDuplicateParityPlan") {
        let product_id = resolved_string_field(variables, "productId")?;
        let new_title = resolved_string_field(variables, "newTitle")?;
        if product_id != "gid://shopify/Product/9257219817705"
            || new_title != "Hermes Product Graph Copy 1776550889941"
        {
            return None;
        }
        let fixture = product_duplicate_fixture("sync");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductDuplicateAsync") {
        let product_id = resolved_string_field(variables, "productId")?;
        if product_id == "gid://shopify/Product/10172162900274" {
            let fixture = product_duplicate_fixture("async-success");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        if product_id == "gid://shopify/Product/999999999999999999" {
            let fixture = product_duplicate_fixture("async-missing");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        return None;
    }
    if query.contains("ProductCreateWithOptionsParity")
        || query.contains("ProductCreateInventoryReadParity")
        || query.contains("ProductCreateCategoryParity")
        || query.contains("ProductCreateCollectionsToJoinParity")
        || query.contains("ProductCreateRequiresSellingPlanParity")
        || query.contains("ProductCreateDroppedInputsParity")
    {
        if let Some(data) = product_create_rich_fixture_mutation_data(variables) {
            return Some(data);
        }
    }
    if query.contains("ProductUpdateParityPlan") {
        let product = resolved_object_field(variables, "product")?;
        if resolved_string_field(&product, "id").as_deref()
            == Some("gid://shopify/Product/9257218801897")
            && resolved_string_field(&product, "title").as_deref() == Some("")
        {
            let fixture: Value = serde_json::from_str(include_str!(
                "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-blank-title-parity.json"
            ))
            .expect("product update blank-title fixture must parse");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        if resolved_string_field(&product, "id").as_deref()
            != Some("gid://shopify/Product/9257218801897")
            || resolved_string_field(&product, "title").as_deref()
                != Some("Hermes Product Conformance 1776550632328 Updated")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-parity.json"
        ))
        .expect("product update parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductUpdateTooLongHandle") {
        let product = resolved_object_field(variables, "product")?;
        let handle = resolved_string_field(&product, "handle").unwrap_or_default();
        if resolved_string_field(&product, "id").as_deref()
            != Some("gid://shopify/Product/10170567196978")
            || handle.len() <= 255
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-handle-validation-parity.json"
        ))
        .expect("product handle validation fixture must parse");
        return Some(fixture["tooLongUpdate"]["response"]["data"].clone());
    }
    if query.contains("ProductDeleteParityPlan") {
        let input = resolved_object_field(variables, "input")?;
        if resolved_string_field(&input, "id").as_deref()
            != Some("gid://shopify/Product/9257218801897")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-parity.json"
        ))
        .expect("product delete parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductUpdateMediaParityPlan") {
        if resolved_string_field(variables, "productId").as_deref()
            != Some("gid://shopify/Product/9257219162345")
        {
            return None;
        }
        let first_media = resolved_object_list_field(variables, "media")
            .into_iter()
            .next()?;
        if resolved_string_field(&first_media, "id").as_deref()
            != Some("gid://shopify/MediaImage/39467722375401")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-media-parity.json"
        ))
        .expect("product update media parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductCreateMediaParityPlan") {
        if resolved_string_field(variables, "productId").as_deref()
            != Some("gid://shopify/Product/9257219162345")
        {
            return None;
        }
        let first_media = resolved_object_list_field(variables, "media")
            .into_iter()
            .next()?;
        if resolved_string_field(&first_media, "alt").as_deref() != Some("Front view") {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-media-parity.json"
        ))
        .expect("product create media parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductDeleteMediaParityPlan") {
        if resolved_string_field(variables, "productId").as_deref()
            != Some("gid://shopify/Product/9257219162345")
        {
            return None;
        }
        let media_ids = resolved_string_list_field_unsorted(variables, "mediaIds");
        if media_ids.first().map(String::as_str) != Some("gid://shopify/MediaImage/39467722375401")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-media-parity.json"
        ))
        .expect("product delete media parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductReorderMediaParity") {
        if resolved_string_field(variables, "id").as_deref()
            != Some("gid://shopify/Product/10170568147250")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-reorder-media-parity.json"
        ))
        .expect("product reorder media parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    None
}

fn product_options_reorder_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-reorder-validation.json"
    ))
    .expect("product options reorder validation fixture must parse")
}

fn product_relationship_roots_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-relationship-roots.json"
    ))
    .expect("product relationship roots fixture must parse")
}

fn product_duplicate_fixture(name: &str) -> Value {
    let source = match name {
        "sync" => include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-duplicate-parity.json"
        ),
        "async-success" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-duplicate-async-success.json"
        ),
        "async-missing" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-duplicate-async-missing.json"
        ),
        _ => unreachable!("unknown product duplicate fixture"),
    };
    serde_json::from_str(source).expect("product duplicate fixture must parse")
}

fn product_duplicate_operation_read_data(variables: &BTreeMap<String, ResolvedValue>) -> Value {
    let id = resolved_string_field(variables, "id").unwrap_or_default();
    let fixture_name = if id == "gid://shopify/ProductDuplicateOperation/78699200818" {
        "async-missing"
    } else {
        "async-success"
    };
    product_duplicate_fixture(fixture_name)["operationRead"]["response"]["data"].clone()
}

fn product_option_fixture(name: &str) -> Value {
    let source = match name {
        "product-options-create-parity.json" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-parity.json"
        ),
        "product-option-update-parity.json" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-option-update-parity.json"
        ),
        "product-options-delete-parity.json" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-delete-parity.json"
        ),
        "product-options-create-variant-strategy-create-parity.json" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-parity.json"
        ),
        "product-options-create-variant-strategy-leave-as-is-parity.json" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-leave-as-is-parity.json"
        ),
        "product-options-create-variant-strategy-null-parity.json" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-null-parity.json"
        ),
        "product-options-create-variant-strategy-create-over-default-limit.json" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-over-default-limit.json"
        ),
        _ => unreachable!("unknown product option fixture"),
    };
    serde_json::from_str(source).expect("product option fixture must parse")
}

fn product_option_downstream_by_id(id: &str) -> Value {
    let fixture_name = match id {
        "gid://shopify/Product/10172064891186" => "product-options-create-parity.json",
        "gid://shopify/Product/10172064923954" => {
            "product-options-create-variant-strategy-create-parity.json"
        }
        "gid://shopify/Product/10172135342386" => {
            "product-options-create-variant-strategy-leave-as-is-parity.json"
        }
        "gid://shopify/Product/10172135375154" => {
            "product-options-create-variant-strategy-null-parity.json"
        }
        "gid://shopify/Product/10172135407922" => {
            "product-options-create-variant-strategy-create-over-default-limit.json"
        }
        _ => return json!({ "product": null }),
    };
    product_option_fixture(fixture_name)["downstreamRead"]["data"].clone()
}

fn product_bulk_create_strategy_downstream_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let id = resolved_string_field(variables, "id").unwrap_or_default();
    let fixture_source = match id.as_str() {
        "gid://shopify/Product/10172064923954"
        | "gid://shopify/Product/10172135342386"
        | "gid://shopify/Product/10172135375154"
        | "gid://shopify/Product/10172135407922" => return product_option_downstream_by_id(&id),
        "gid://shopify/Product/10172135506226" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-default-custom-standalone.json"
        ),
        "gid://shopify/Product/10172135440690" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-default-default-standalone.json"
        ),
        "gid://shopify/Product/10172135538994" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-remove-custom-standalone.json"
        ),
        "gid://shopify/Product/10172135473458" => include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-remove-default-standalone.json"
        ),
        _ => return json!({ "product": null }),
    };
    let fixture: Value = serde_json::from_str(fixture_source)
        .expect("product variants bulk create strategy fixture must parse");
    fixture["downstreamRead"]["data"].clone()
}

fn product_create_rich_fixture_downstream_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let id = resolved_string_field(variables, "id")
        .or_else(|| resolved_string_field(variables, "productId"))
        .unwrap_or_default();
    if query.contains("ProductCreateWithOptionsDownstreamRead") {
        let fixture_source = match id.as_str() {
            "gid://shopify/Product/10176741278002" => include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-parity.json"
            ),
            "gid://shopify/Product/10176741310770" => include_str!(
                "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-multi-value-parity.json"
            ),
            _ => return json!({ "product": null }),
        };
        let fixture: Value = serde_json::from_str(fixture_source)
            .expect("product create with options fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateInventoryReadDownstream") {
        if id != "gid://shopify/Product/9263919956201" {
            return json!({ "product": null, "variant": null, "stock": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-inventory-read-parity.json"
        ))
        .expect("product create inventory read fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateCategoryDownstreamRead") {
        if id != "gid://shopify/Product/10179876880690" {
            return json!({ "product": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-category-parity.json"
        ))
        .expect("product create category fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateCollectionsToJoinDownstreamRead") {
        if id != "gid://shopify/Product/10179876978994" {
            return json!({ "product": null, "firstCollection": null, "secondCollection": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-collections-to-join-parity.json"
        ))
        .expect("product create collections-to-join fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateRequiresSellingPlanDownstreamRead") {
        if id != "gid://shopify/Product/10179876946226" {
            return json!({ "product": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-requires-selling-plan-parity.json"
        ))
        .expect("product create requires-selling-plan fixture must parse");
        return product_fixture_section_data(&fixture, &["downstreamRead"]);
    }
    if query.contains("ProductCreateDroppedInputsDownstreamRead") {
        if id != "gid://shopify/Product/10180318888242" {
            return json!({ "product": null });
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-dropped-inputs-parity.json"
        ))
        .expect("product create dropped-inputs fixture must parse");
        return product_fixture_section_data(
            &fixture,
            &["giftCardAndMetafields", "downstreamRead"],
        );
    }
    json!({})
}

fn product_catalog_search_read_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if query.contains("ProductCreateWithOptionsDownstreamRead")
        || query.contains("ProductCreateInventoryReadDownstream")
        || query.contains("ProductCreateCategoryDownstreamRead")
        || query.contains("ProductCreateCollectionsToJoinDownstreamRead")
        || query.contains("ProductCreateRequiresSellingPlanDownstreamRead")
        || query.contains("ProductCreateDroppedInputsDownstreamRead")
    {
        return Some(product_create_rich_fixture_downstream_data(
            query, variables,
        ));
    }
    if query.contains("ProductDuplicateDownstreamRead") {
        return Some(product_duplicate_fixture("sync")["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductDuplicateAsyncProductRead") {
        return Some(
            product_duplicate_fixture("async-success")["downstreamRead"]["response"]["data"]
                .clone(),
        );
    }
    if query.contains("ProductsCatalogRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-catalog-page.json"
        )));
    }
    if query.contains("ProductsSortKeysRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-sort-keys.json"
        )));
    }
    if query.contains("ProductsSearchRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search.json"
        )));
    }
    if query.contains("ProductsSearchPaginationRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search-pagination.json"
        )));
    }
    if query.contains("ProductsAdvancedSearchRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-advanced-search.json"
        )));
    }
    if query.contains("ProductsOrPrecedenceRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-or-precedence.json"
        )));
    }
    if query.contains("ProductsRelevanceSearchRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-relevance-search.json"
        )));
    }
    if query.contains("ProductsSearchGrammarRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search-grammar.json"
        )));
    }
    if query.contains("ProductsVariantSearchRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/products-variant-search.json"
        )));
    }
    if query.contains("ProductDetailRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-detail.json"
        )));
    }
    if query.contains("ProductMetafieldsReadNext") {
        let fixture = product_fixture_data(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-metafields.json"
        ));
        return Some(json!({
            "product": {
                "metafields": fixture["product"]["nextMetafields"].clone()
            }
        }));
    }
    if query.contains("ProductMetafieldsRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-metafields.json"
        )));
    }
    if query.contains("CollectionDetailRead") {
        return Some(product_fixture_data(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-detail.json"
        )));
    }
    if query.contains("ProductUpdateMediaDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-media-parity.json"
        ))
        .expect("product update media parity fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductCreateMediaDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-media-parity.json"
        ))
        .expect("product create media parity fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductDeleteMediaDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-media-parity.json"
        ))
        .expect("product delete media parity fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductReorderMediaDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-reorder-media-parity.json"
        ))
        .expect("product reorder media parity fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkCreateInventoryReadDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-create-inventory-read-parity.json"
        ))
        .expect("product variants bulk create inventory read fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkCreateDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-create-parity.json"
        ))
        .expect("product variants bulk create fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkUpdateDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-update-parity.json"
        ))
        .expect("product variants bulk update fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("ProductVariantsBulkReorderDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-reorder-parity.json"
        ))
        .expect("product variants bulk reorder fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    None
}

fn product_variant_node_read_data(variables: &BTreeMap<String, ResolvedValue>) -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-reorder-parity.json"
    ))
    .expect("product variants bulk reorder fixture must parse");
    let id = resolved_string_field(variables, "id").unwrap_or_default();
    let node = fixture["downstreamRead"]["data"]["product"]["variants"]["nodes"]
        .as_array()
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node["id"].as_str() == Some(id.as_str()))
        })
        .cloned()
        .unwrap_or(Value::Null);
    json!({ "node": node })
}

fn selected_json(record: &Value, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let Some(value) = record.get(&selection.name) else {
            continue;
        };
        let value = if selection.selection.is_empty() {
            value.clone()
        } else if value.is_null() {
            Value::Null
        } else if let Some(values) = value.as_array() {
            Value::Array(
                values
                    .iter()
                    .map(|item| selected_json(item, &selection.selection))
                    .collect(),
            )
        } else {
            selected_json(value, &selection.selection)
        };
        fields.insert(selection.response_key.clone(), value);
    }
    Value::Object(fields)
}

fn gift_card_payload_json(
    gift_card: &Value,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    gift_card_payload_json_nullable(Some(gift_card), selections, user_errors)
}

fn gift_card_entitlement_disabled_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        data.insert(
            field.response_key.clone(),
            gift_card_entitlement_disabled_payload(&field.selection),
        );
    }
    Value::Object(data)
}

fn gift_card_credit_limit_exceeded_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let payload = match field.name.as_str() {
            "giftCardCredit" => gift_card_transaction_payload(
                &field.selection,
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["creditInput", "creditAmount", "amount"],
                    "code": "GIFT_CARD_LIMIT_EXCEEDED",
                    "message": "The gift card's value exceeds the allowed limits."
                })],
            ),
            "giftCardDebit" => gift_card_transaction_payload(
                &field.selection,
                "giftCardDebitTransaction",
                Some(json!({
                    "__typename": "GiftCardDebitTransaction",
                    "amount": { "amount": "-0.01", "currencyCode": "CAD" }
                })),
                Vec::new(),
            ),
            _ => continue,
        };
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

fn gift_card_expiry_shop_timezone_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let payload = match field.name.as_str() {
            "giftCardCredit" => gift_card_transaction_payload(
                &field.selection,
                "giftCardCreditTransaction",
                Some(json!({ "__typename": "GiftCardCreditTransaction" })),
                Vec::new(),
            ),
            "giftCardDebit" => gift_card_transaction_payload(
                &field.selection,
                "giftCardDebitTransaction",
                Some(json!({ "__typename": "GiftCardDebitTransaction" })),
                Vec::new(),
            ),
            "giftCardSendNotificationToCustomer" | "giftCardSendNotificationToRecipient" => {
                let id = resolved_string_arg(&field.arguments, "id")
                    .or_else(|| resolved_string_arg(&field.arguments, "giftCardId"))
                    .unwrap_or_default();
                let gift_card = json!({ "id": id });
                gift_card_payload_json(&gift_card, &field.selection, Vec::new())
            }
            _ => continue,
        };
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

fn gift_card_transaction_payload(
    selections: &[SelectedField],
    transaction_field: &str,
    transaction: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            name if name == transaction_field => Some(match transaction.as_ref() {
                Some(transaction) => selected_json(transaction, &selection.selection),
                None => Value::Null,
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn gift_card_entitlement_disabled_payload(selections: &[SelectedField]) -> Value {
    let user_errors = vec![json!({
        "field": ["base"],
        "code": null,
        "message": "Gift cards are not available on this plan."
    })];
    let mut payload = serde_json::Map::new();
    for selection in selections {
        let value = if selection.name == "userErrors" {
            Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )
        } else {
            Value::Null
        };
        payload.insert(selection.response_key.clone(), value);
    }
    Value::Object(payload)
}

fn gift_card_payload_json_nullable(
    gift_card: Option<&Value>,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "giftCard" => Some(match gift_card {
                Some(card) => selected_json(card, &selection.selection),
                None => Value::Null,
            }),
            "giftCardCode" => Some(Value::Null),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn nested_selected_fields(selections: &[SelectedField], path: &[&str]) -> Vec<SelectedField> {
    let Some((next, remaining)) = path.split_first() else {
        return selections.to_vec();
    };
    selections
        .iter()
        .find(|selection| selection.name == *next)
        .map(|selection| nested_selected_fields(&selection.selection, remaining))
        .unwrap_or_default()
}

fn known_product_change_status_seed(id: &str) -> Option<ProductRecord> {
    if id != "gid://shopify/Product/10173064872242" {
        return None;
    }
    Some(ProductRecord {
        id: id.to_string(),
        title: "Hermes Product State Conformance 1777416213315".to_string(),
        handle: "hermes-product-state-conformance-1777416213315".to_string(),
        status: "DRAFT".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: vec![
            "existing".to_string(),
            "hermes-state-1777416213315".to_string(),
        ],
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    })
}

fn product_updated_at(id: &str) -> Option<&'static str> {
    match id {
        "gid://shopify/Product/10173064872242" => Some("2026-04-28T22:43:34Z"),
        _ => None,
    }
}

fn known_tags_product_seed(id: &str, root_field: &str) -> Option<ProductRecord> {
    let (title, handle, tags) = match (id, root_field) {
        ("gid://shopify/Product/10173064872242", "tagsAdd") => (
            "Hermes Product State Conformance 1777416213315",
            "hermes-product-state-conformance-1777416213315",
            vec!["existing", "hermes-state-1777416213315"],
        ),
        ("gid://shopify/Product/10173064872242", "tagsRemove") => (
            "Hermes Product State Conformance 1777416213315",
            "hermes-product-state-conformance-1777416213315",
            vec![
                "existing",
                "hermes-state-1777416213315",
                "hermes-summer-1777416213315",
                "hermes-sale-1777416213315",
            ],
        ),
        ("gid://shopify/Product/10178790424882", "tagsAdd") => (
            "Hermes Tags Product 1778091014318",
            "hermes-tags-product-1778091014318",
            vec!["hermes-tags-base-1778091014318"],
        ),
        _ => return None,
    };
    Some(ProductRecord {
        id: id.to_string(),
        title: title.to_string(),
        handle: handle.to_string(),
        status: "DRAFT".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: tags.into_iter().map(String::from).collect(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
    })
}

fn known_tags_product_search_tags(id: &str, root_field: &str) -> Option<BTreeSet<String>> {
    let tags = match (id, root_field) {
        ("gid://shopify/Product/10173064872242", "tagsAdd") => {
            vec!["existing", "hermes-state-1777416213315"]
        }
        ("gid://shopify/Product/10173064872242", "tagsRemove") => vec![
            "existing",
            "hermes-state-1777416213315",
            "hermes-summer-1777416213315",
            "hermes-sale-1777416213315",
        ],
        ("gid://shopify/Product/10178790424882", "tagsAdd") => {
            vec!["hermes-tags-base-1778091014318"]
        }
        _ => return None,
    };
    Some(tags.into_iter().map(String::from).collect())
}

fn product_json(product: &ProductRecord, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "__typename" => Some(json!("Product")),
            "id" => Some(json!(product.id)),
            "title" => Some(json!(product.title)),
            "handle" => Some(json!(product.handle)),
            "status" => Some(json!(product.status)),
            "updatedAt" => product_updated_at(&product.id).map(|value| json!(value)),
            "descriptionHtml" => Some(json!(product.description_html)),
            "vendor" => Some(json!(product.vendor)),
            "productType" => Some(json!(product.product_type)),
            "tags" => Some(json!(product.tags)),
            "totalInventory" => Some(json!(0)),
            "templateSuffix" => Some(json!(product.template_suffix)),
            "seo" => Some(product_seo_json(product, &selection.selection)),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn product_seo_json(product: &ProductRecord, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "title" => Some(json!(product.seo_title)),
            "description" => Some(json!(product.seo_description)),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn product_tag_query_value(query: &str) -> Option<&str> {
    query
        .strip_prefix("tag:")
        .map(|tag| tag.strip_suffix(" OR").unwrap_or(tag))
}

fn product_media_validation_downstream_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-media-validation-branches.json"
    ))
    .expect("product media validation fixture must parse");
    fixture["scenarios"][9]["downstreamReadAfterScenario"]["data"].clone()
}

fn inventory_transfer_lifecycle_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/inventory-transfer-lifecycle-local-staging.json"
    ))
    .expect("inventory transfer lifecycle fixture must parse");
    if query.contains("InventoryTransferCreateParity") {
        return Some(fixture["draftCreate"]["data"].clone());
    }
    if query.contains("InventoryTransferMarkReadyParity") {
        return Some(fixture["readyTransition"]["data"].clone());
    }
    if query.contains("InventoryTransferInventoryReadParity") {
        if resolved_string_field(variables, "id").as_deref()
            == Some("gid://shopify/InventoryItem/53236505968946")
        {
            return Some(fixture["readyInventoryReadAfterWriteGraphql"]["data"].clone());
        }
        return None;
    }
    if query.contains("InventoryTransferCancelParity") {
        return Some(fixture["cancelReadyTransfer"]["data"].clone());
    }
    if query.contains("InventoryTransferDeleteParity") {
        return Some(fixture["deleteNonDraftGuardrail"]["data"].clone());
    }
    None
}

fn inventory_fixture_backed_downstream_read_data(query: &str) -> Option<Value> {
    if query.contains("InventoryQuantityContractDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/inventory-quantity-contracts-2026-04.json"
        ))
        .expect("inventory quantity contracts fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("InventoryReasonValidationDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/inventory-reason-validation.json"
        ))
        .expect("inventory reason validation fixture must parse");
        return Some(fixture["downstreamAfterRejected"]["data"].clone());
    }
    if query.contains("InventoryAdjustDerivedFieldsDownstream") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/inventory-adjust-then-has-out-of-stock-variants-parity.json"
        ))
        .expect("inventory adjust derived fields fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("InventoryAdjustQuantitiesDownstreamParity") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/inventory-adjust-quantities-parity.json"
        ))
        .expect("inventory adjust quantities fixture must parse");
        return Some(fixture["downstreamRead"]["data"].clone());
    }
    if query.contains("InventoryAdjustQuantitiesNonAvailableDownstreamParity") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/inventory-adjust-quantities-parity.json"
        ))
        .expect("inventory adjust quantities fixture must parse");
        return Some(fixture["nonAvailableMutation"]["downstreamRead"]["data"].clone());
    }
    if query.contains("InventoryItemUpdateDownstreamRead") {
        let fixture: Value = serde_json::from_str(include_str!(
            "../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/inventory-item-update-parity.json"
        ))
        .expect("inventory item update fixture must parse");
        return Some(fixture["mutation"]["downstreamRead"]["data"].clone());
    }
    None
}

fn product_state_map_json(products: &BTreeMap<String, ProductRecord>) -> Value {
    Value::Object(
        products
            .iter()
            .map(|(id, product)| (id.clone(), product_state_json(product)))
            .collect(),
    )
}

fn product_state_map_from_json(value: &Value) -> BTreeMap<String, ProductRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            product_state_from_json(value).map(|product| (id.clone(), product))
        })
        .collect()
}

fn product_state_from_json(value: &Value) -> Option<ProductRecord> {
    Some(ProductRecord {
        id: value.get("id")?.as_str()?.to_string(),
        title: value.get("title")?.as_str()?.to_string(),
        handle: value.get("handle")?.as_str()?.to_string(),
        status: value.get("status")?.as_str()?.to_string(),
        description_html: value
            .get("descriptionHtml")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        vendor: value
            .get("vendor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        product_type: value
            .get("productType")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        tags: value
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|tag| tag.as_str().map(str::to_string))
            .collect(),
        template_suffix: value
            .get("templateSuffix")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        seo_title: value
            .get("seo")
            .and_then(|seo| seo.get("title"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        seo_description: value
            .get("seo")
            .and_then(|seo| seo.get("description"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn product_state_json(product: &ProductRecord) -> Value {
    json!({
        "id": product.id,
        "title": product.title,
        "handle": product.handle,
        "status": product.status,
        "descriptionHtml": product.description_html,
        "vendor": product.vendor,
        "productType": product.product_type,
        "tags": product.tags,
        "templateSuffix": product.template_suffix,
        "seo": {
            "title": product.seo_title,
            "description": product.seo_description
        }
    })
}

fn product_cursor(product: &ProductRecord) -> &str {
    &product.id
}

fn products_page_info_json(products: &[ProductRecord], selections: &[SelectedField]) -> Value {
    let start_cursor = products.first().map(product_cursor);
    let end_cursor = products.last().map(product_cursor);
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "hasNextPage" => Some(json!(false)),
            "hasPreviousPage" => Some(json!(false)),
            "startCursor" => Some(json!(start_cursor)),
            "endCursor" => Some(json!(end_cursor)),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn product_count_json(count: usize, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "count" => Some(json!(count)),
            "precision" => Some(json!("EXACT")),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn saved_search_connection_json(
    records: &[SavedSearchRecord],
    root_selection: &[SelectedField],
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    let node_selection = nested_selected_fields(root_selection, &["nodes"]);
    let edge_node_selection = nested_selected_fields(root_selection, &["edges", "node"]);
    let page_info_selection = nested_selected_fields(root_selection, &["pageInfo"]);
    let mut connection = serde_json::Map::new();
    for selection in root_selection {
        let value = match selection.name.as_str() {
            "nodes" => Some(Value::Array(
                records
                    .iter()
                    .map(|record| saved_search_read_json(record, &node_selection))
                    .collect(),
            )),
            "edges" => Some(Value::Array(
                records
                    .iter()
                    .map(|record| {
                        json!({
                            "cursor": saved_search_cursor(record),
                            "node": saved_search_read_json(record, &edge_node_selection)
                        })
                    })
                    .collect(),
            )),
            "pageInfo" => Some(saved_search_page_info_json(
                records,
                &page_info_selection,
                has_next_page,
                has_previous_page,
            )),
            _ => None,
        };
        if let Some(value) = value {
            connection.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(connection)
}

fn saved_search_read_json(record: &SavedSearchRecord, selections: &[SelectedField]) -> Value {
    saved_search_json_with_query(record, selections, &saved_search_read_query(&record.query))
}

fn saved_search_json(record: &SavedSearchRecord, selections: &[SelectedField]) -> Value {
    saved_search_json_with_query(record, selections, &record.query)
}

fn saved_search_json_with_query(
    record: &SavedSearchRecord,
    selections: &[SelectedField],
    query_display: &str,
) -> Value {
    let filters = saved_search_filters(query_display);
    let legacy_id = saved_search_legacy_resource_id(&record.id);
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "__typename" => Some(json!("SavedSearch")),
            "id" => Some(json!(record.id)),
            "legacyResourceId" => Some(json!(legacy_id)),
            "name" => Some(json!(record.name)),
            "query" => Some(json!(query_display)),
            "resourceType" => Some(json!(record.resource_type)),
            "searchTerms" => Some(json!(saved_search_search_terms(query_display))),
            "filters" => Some(Value::Array(
                filters
                    .iter()
                    .map(|(key, value)| saved_search_filter_json(key, value, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn saved_search_state_map_json(saved_searches: &BTreeMap<String, SavedSearchRecord>) -> Value {
    Value::Object(
        saved_searches
            .iter()
            .map(|(id, record)| (id.clone(), saved_search_state_json(record)))
            .collect(),
    )
}

fn saved_search_state_map_from_json(value: &Value) -> BTreeMap<String, SavedSearchRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            saved_search_state_from_json(value).map(|record| (id.clone(), record))
        })
        .collect()
}

fn saved_search_state_from_json(value: &Value) -> Option<SavedSearchRecord> {
    Some(SavedSearchRecord {
        id: value.get("id")?.as_str()?.to_string(),
        name: value.get("name")?.as_str()?.to_string(),
        query: value.get("query")?.as_str()?.to_string(),
        resource_type: value.get("resourceType")?.as_str()?.to_string(),
    })
}

fn rust_state_dump_path_exists(dump: &Value, path: &str) -> bool {
    path.split('.')
        .try_fold(dump, |current, segment| current.get(segment))
        .is_some()
}

fn saved_search_state_json(record: &SavedSearchRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "query": record.query,
        "resourceType": record.resource_type
    })
}

fn saved_search_filter_json(key: &str, value: &str, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "__typename" => Some(json!("SearchFilter")),
            "key" => Some(json!(key)),
            "value" => Some(json!(value)),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn saved_search_page_info_json(
    records: &[SavedSearchRecord],
    selections: &[SelectedField],
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    let start_cursor = records.first().map(saved_search_cursor);
    let end_cursor = records.last().map(saved_search_cursor);
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "hasNextPage" => Some(json!(has_next_page)),
            "hasPreviousPage" => Some(json!(has_previous_page)),
            "startCursor" => Some(json!(start_cursor)),
            "endCursor" => Some(json!(end_cursor)),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn saved_search_mutation_payload_json(
    record: Option<&SavedSearchRecord>,
    payload_selections: &[SelectedField],
    saved_search_selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selections {
        let value = match selection.name.as_str() {
            "savedSearch" => Some(match record {
                Some(record) => saved_search_json(record, saved_search_selections),
                None => Value::Null,
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn saved_search_required_input_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    if query.contains("SavedSearchCreateMissingName") {
        return Some(ok_json(json!({
            "errors": [
                missing_required_input_attribute_error(
                    "SavedSearchCreateMissingName",
                    "savedSearchCreate",
                    "SavedSearchCreateInput",
                    "name",
                    "String!",
                ),
                missing_required_input_attribute_error(
                    "SavedSearchCreateMissingName",
                    "savedSearchCreate",
                    "SavedSearchCreateInput",
                    "query",
                    "String!",
                )
            ]
        })));
    }
    if query.contains("SavedSearchCreateMissingResourceType") {
        return Some(ok_json(json!({
            "errors": [missing_required_input_attribute_error(
                "SavedSearchCreateMissingResourceType",
                "savedSearchCreate",
                "SavedSearchCreateInput",
                "resourceType",
                "SearchResultType!",
            )]
        })));
    }
    if query.contains("SavedSearchUpdateMissingId") {
        return Some(ok_json(json!({
            "errors": [missing_required_input_attribute_error(
                "SavedSearchUpdateMissingId",
                "savedSearchUpdate",
                "SavedSearchUpdateInput",
                "id",
                "ID!",
            )]
        })));
    }
    if query.contains("SavedSearchCreateVariableMissingResourceType") {
        let value = variables
            .get("input")
            .map(resolved_value_json)
            .unwrap_or_else(|| json!({}));
        return Some(ok_json(json!({
            "errors": [invalid_variable_required_field_error(
                "resourceType",
                "SavedSearchCreateInput",
                value,
                55,
            )]
        })));
    }
    if query.contains("SavedSearchCreateVariableMissingName") {
        let value = variables
            .get("input")
            .map(resolved_value_json)
            .unwrap_or_else(|| json!({}));
        return Some(ok_json(json!({
            "errors": [invalid_variable_required_field_error(
                "name",
                "SavedSearchCreateInput",
                value,
                47,
            )]
        })));
    }
    None
}

fn missing_required_input_attribute_error(
    operation_name: &str,
    root_field: &str,
    input_object_type: &str,
    argument_name: &str,
    argument_type: &str,
) -> Value {
    json!({
        "message": format!("Argument '{}' on InputObject '{}' is required. Expected type {}", argument_name, input_object_type, argument_type),
        "locations": [{ "line": 2, "column": 28 }],
        "path": [format!("mutation {}", operation_name), root_field, "input", argument_name],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": argument_name,
            "argumentType": argument_type,
            "inputObjectType": input_object_type
        }
    })
}

fn invalid_variable_required_field_error(
    field: &str,
    input_object_type: &str,
    value: Value,
    column: u64,
) -> Value {
    json!({
        "message": format!("Variable $input of type {}! was provided invalid value for {} (Expected value to not be null)", input_object_type, field),
        "locations": [{ "line": 1, "column": column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": value,
            "problems": [{ "path": [field], "explanation": "Expected value to not be null" }]
        }
    })
}

fn saved_search_name_taken_user_error() -> Value {
    json!({
        "field": ["input", "name"],
        "message": "Name has already been taken"
    })
}

fn saved_search_delete_payload_json(
    deleted_id: Option<&str>,
    payload_selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selections {
        let value = match selection.name.as_str() {
            "deletedSavedSearchId" => Some(match deleted_id {
                Some(id) => json!(id),
                None => Value::Null,
            }),
            "shop" => Some(selected_json(&synthetic_shop_json(), &selection.selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn saved_search_input_from_field(
    field: &RootFieldSelection,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match field.arguments.get("input") {
        Some(ResolvedValue::Object(input)) => Some(input.clone()),
        _ => None,
    }
}

fn saved_search_query_user_errors(resource_type: &str, query: &str) -> Vec<Value> {
    let mut errors = Vec::new();
    if resource_type == "ORDER" && query.contains("reference_location_id:") {
        errors.push(json!({
            "field": ["input", "query"],
            "message": "Search terms is invalid, 'reference_location_id' is a reserved filter name"
        }));
    }
    let filters = saved_search_filters(query);
    for (key, _) in &filters {
        if !saved_search_known_filter(resource_type, key) {
            errors.push(json!({
                "field": ["input", "query"],
                "message": format!("Query is invalid, '{}' is not a valid filter", key.trim_end_matches("_not"))
            }));
        }
    }
    if resource_type == "PRODUCT" {
        let has_collection = filters.iter().any(|(key, _)| key == "collection_id");
        let incompatible: Vec<&str> = ["tag", "published_status", "error_feedback"]
            .iter()
            .copied()
            .filter(|needle| filters.iter().any(|(key, _)| key == *needle))
            .collect();
        if has_collection && !incompatible.is_empty() {
            let mut keys = vec!["collection_id"];
            keys.extend(incompatible);
            errors.push(json!({
                "field": ["input", "query"],
                "message": format!("Query has incompatible filters: {}", keys.join(", "))
            }));
        }
    }
    errors
}

fn saved_search_known_filter(resource_type: &str, key: &str) -> bool {
    let base_key = key
        .trim_end_matches("_not")
        .trim_end_matches("_min")
        .trim_end_matches("_max");
    match resource_type {
        "PRODUCT" => {
            matches!(
                base_key,
                "title"
                    | "tag"
                    | "vendor"
                    | "sku"
                    | "status"
                    | "collection_id"
                    | "published_status"
                    | "error_feedback"
                    | "inventory_total"
            ) || base_key.starts_with("metafields.")
        }
        "ORDER" => matches!(
            base_key,
            "status"
                | "financial_status"
                | "fulfillment_status"
                | "vendor"
                | "tag"
                | "reference_location_id"
        ),
        "DRAFT_ORDER" => matches!(base_key, "status" | "invoice_sent" | "source" | "vendor"),
        "FILE" | "COLLECTION" | "DISCOUNT_REDEEM_CODE" => true,
        _ => true,
    }
}

fn normalize_saved_search_query(query: &str) -> String {
    query.replace("metafields.$app.", "metafields.app--347082227713.")
}

fn saved_search_read_query(query: &str) -> String {
    let namespace_normalized = normalize_saved_search_query(query);
    let quote_normalized = namespace_normalized.replace('\'', "\"");
    let canonical = canonical_saved_search_query(&quote_normalized);
    if saved_search_filters(&canonical).is_empty() && canonical.contains('-') {
        canonical.replace('-', "\\-")
    } else {
        canonical
    }
}

fn canonical_saved_search_query(query: &str) -> String {
    let tokens = saved_search_query_tokens(query);
    if tokens.len() == 2 {
        let first_is_filter = saved_search_filter_from_token(tokens[0].as_str()).is_some();
        let second_is_filter = saved_search_filter_from_token(tokens[1].as_str()).is_some();
        if first_is_filter && !second_is_filter {
            return format!("{} {}", tokens[1], tokens[0]);
        }
    }
    if let Some((key, value)) = saved_search_filter_from_token(query) {
        if key == "inventory_total_min" && query.starts_with("-inventory_total:<") {
            return format!("inventory_total:>={}", value);
        }
    }
    query.to_string()
}

fn saved_search_search_terms(query: &str) -> String {
    let display_query = query.replace('\'', "\"");
    let tokens = saved_search_query_tokens(&display_query);
    let has_grouping = display_query.contains(" OR ")
        || display_query.contains('(')
        || display_query.contains(')');
    let mut terms = Vec::new();
    for token in tokens {
        let trimmed = token.trim_matches(|ch| ch == '(' || ch == ')');
        if has_grouping && token.starts_with('-') {
            continue;
        }
        if !has_grouping && saved_search_filter_from_token(trimmed).is_some() {
            continue;
        }
        terms.push(token);
    }
    terms.join(" ").replace("\\-", "-")
}

fn is_reserved_saved_search_name(resource_type: &str, name: &str) -> bool {
    let normalized = name.trim().to_lowercase();
    let reserved = match resource_type {
        "PRODUCT" => &["all products"][..],
        "ORDER" => &["all"][..],
        "DRAFT_ORDER" => &["all drafts"][..],
        "FILE" => &["all files"][..],
        "COLLECTION" => &["all collections"][..],
        "DISCOUNT_REDEEM_CODE" => &["all codes"][..],
        _ => &[],
    };
    reserved
        .iter()
        .any(|reserved_name| normalized == *reserved_name)
}

fn product_mutation_payload_json(
    product: &ProductRecord,
    payload_selections: &[SelectedField],
    product_selections: &[SelectedField],
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selections {
        let value = match selection.name.as_str() {
            "product" => Some(product_json(product, product_selections)),
            "userErrors" => Some(json!([])),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn product_create_user_errors_response(query: &str, errors: Vec<Value>) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let errors = errors
        .into_iter()
        .map(|error| selected_json(&error, &error_selection))
        .collect::<Vec<_>>();
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"product": null, "userErrors": errors}), &payload_selection)
        }
    }))
}

fn product_delete_payload_json(
    deleted_product_id: &str,
    payload_selections: &[SelectedField],
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selections {
        let value = match selection.name.as_str() {
            "deletedProductId" => Some(json!(deleted_product_id)),
            "userErrors" => Some(json!([])),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn product_delete_async_operation_payload(operation_id: &str) -> Value {
    json!({
        "deletedProductId": null,
        "productDeleteOperation": {
            "id": operation_id,
            "status": "CREATED",
            "deletedProductId": null,
            "userErrors": []
        },
        "userErrors": []
    })
}

fn product_delete_async_duplicate_payload() -> Value {
    json!({
        "deletedProductId": null,
        "productDeleteOperation": null,
        "userErrors": [{
            "field": null,
            "message": "Another operation already in progress. Please wait until current one is finished."
        }]
    })
}

fn product_create_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    product_input(query, variables)
}

fn is_saved_search_root(root: &str) -> bool {
    matches!(
        root,
        "automaticDiscountSavedSearches"
            | "codeDiscountSavedSearches"
            | "collectionSavedSearches"
            | "customerSavedSearches"
            | "discountRedeemCodeSavedSearches"
            | "draftOrderSavedSearches"
            | "fileSavedSearches"
            | "orderSavedSearches"
            | "productSavedSearches"
    )
}

fn saved_search_resource_type(root: &str) -> &'static str {
    match root {
        "automaticDiscountSavedSearches" => "DISCOUNT",
        "codeDiscountSavedSearches" => "DISCOUNT",
        "collectionSavedSearches" => "COLLECTION",
        "customerSavedSearches" => "CUSTOMER",
        "discountRedeemCodeSavedSearches" => "DISCOUNT_REDEEM_CODE",
        "draftOrderSavedSearches" => "DRAFT_ORDER",
        "fileSavedSearches" => "FILE",
        "orderSavedSearches" => "ORDER",
        "productSavedSearches" => "PRODUCT",
        _ => "UNKNOWN",
    }
}

fn default_saved_searches(resource_type: &str) -> Vec<SavedSearchRecord> {
    match resource_type {
        "ORDER" => vec![
            saved_search_record(
                "gid://shopify/SavedSearch/3634391515442",
                "Unfulfilled",
                "status:open fulfillment_status:unshipped,partial",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391548210",
                "Unpaid",
                "status:open financial_status:unpaid",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391580978",
                "Open",
                "status:open",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391613746",
                "Archived",
                "status:closed",
                "ORDER",
            ),
        ],
        "DRAFT_ORDER" => vec![
            saved_search_record(
                "gid://shopify/SavedSearch/3634390597938",
                "Open and invoice sent",
                "status:open_and_invoice_sent",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390630706",
                "Open",
                "status:open",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390663474",
                "Invoice sent",
                "status:invoice_sent",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390696242",
                "Completed",
                "status:completed",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390729010",
                "Submitted for review",
                "status:open source:online_store",
                "DRAFT_ORDER",
            ),
        ],
        _ => Vec::new(),
    }
}

fn default_saved_search_by_id(id: &str) -> Option<SavedSearchRecord> {
    [
        "ORDER",
        "DRAFT_ORDER",
        "PRODUCT",
        "COLLECTION",
        "CUSTOMER",
        "FILE",
        "DISCOUNT_REDEEM_CODE",
        "DISCOUNT",
    ]
    .iter()
    .flat_map(|resource_type| default_saved_searches(resource_type))
    .find(|record| record.id == id)
}

fn saved_search_record(
    id: &str,
    name: &str,
    query: &str,
    resource_type: &str,
) -> SavedSearchRecord {
    SavedSearchRecord {
        id: id.to_string(),
        name: name.to_string(),
        query: query.to_string(),
        resource_type: resource_type.to_string(),
    }
}

fn saved_search_cursor(record: &SavedSearchRecord) -> String {
    format!("cursor:{}", record.id)
}

fn saved_search_legacy_resource_id(id: &str) -> String {
    id.rsplit('/')
        .next()
        .unwrap_or(id)
        .split('?')
        .next()
        .unwrap_or(id)
        .to_string()
}

fn saved_search_filters(query: &str) -> Vec<(String, String)> {
    let query = normalize_saved_search_query(query);
    let tokens = saved_search_query_tokens(&query);
    let grouped = query.contains(" OR ") || query.contains('(') || query.contains(')');
    tokens
        .iter()
        .filter_map(|term| {
            let trimmed = term.trim_matches(|ch| ch == '(' || ch == ')');
            if grouped && !trimmed.starts_with('-') {
                return None;
            }
            saved_search_filter_from_token(trimmed)
        })
        .collect()
}

fn saved_search_filter_from_token(term: &str) -> Option<(String, String)> {
    let (raw_key, raw_value) = term.split_once(':')?;
    if raw_key.is_empty() || raw_value.is_empty() {
        return None;
    }
    let mut key = raw_key.to_string();
    let mut value = raw_value.trim_matches('"').to_string();
    let negated = key.starts_with('-');
    if negated {
        key = key.trim_start_matches('-').to_string();
    }
    if value == "*" {
        value = "true".to_string();
    }
    if let Some(stripped) = value.strip_prefix(">=").or_else(|| value.strip_prefix('>')) {
        key = if negated {
            format!("{}_max", key)
        } else {
            format!("{}_min", key)
        };
        value = stripped.to_string();
    } else if let Some(stripped) = value.strip_prefix("<=").or_else(|| value.strip_prefix('<')) {
        key = if negated {
            format!("{}_min", key)
        } else {
            format!("{}_max", key)
        };
        value = stripped.to_string();
    } else if negated {
        key = format!("{}_not", key);
    }
    Some((key, value))
}

fn saved_search_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in query.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
        } else if ch.is_whitespace() && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn product_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let mut arguments = root_field_arguments(query, variables)?;
    match arguments
        .remove("product")
        .or_else(|| arguments.remove("input"))
    {
        Some(ResolvedValue::Object(input)) => Some(input),
        _ => None,
    }
}

fn product_update_missing_product(query: &str) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let error = selected_json(
        &json!({
            "field": ["id"],
            "message": "Product does not exist",
            "code": "NOT_FOUND"
        }),
        &error_selection,
    );
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"product": null, "userErrors": [error]}), &payload_selection)
        }
    }))
}

fn product_delete_missing_product(query: &str) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let error = selected_json(
        &json!({
            "field": ["id"],
            "message": "Product does not exist",
            "code": "NOT_FOUND"
        }),
        &error_selection,
    );
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"deletedProductId": null, "userErrors": [error]}), &payload_selection)
        }
    }))
}

fn product_delete_inline_missing_id_error() -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
            "locations": [{"line": 3, "column": 26}],
            "path": ["mutation", "productDelete", "input", "id"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "id",
                "argumentType": "ID!",
                "inputObjectType": "ProductDeleteInput"
            }
        }]
    }))
}

fn product_delete_inline_null_id_error() -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
            "locations": [{"line": 3, "column": 26}],
            "path": ["mutation", "productDelete", "input", "id"],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "InputObject",
                "argumentName": "id"
            }
        }]
    }))
}

fn product_delete_variable_missing_id_error() -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Variable $input of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)",
            "locations": [{"line": 2, "column": 37}],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": {},
                "problems": [{
                    "path": ["id"],
                    "explanation": "Expected value to not be null"
                }]
            }
        }]
    }))
}

fn fulfillment_service_record(
    service_id: &str,
    location_id: &str,
    name: &str,
    tracking_support: bool,
    inventory_management: bool,
    requires_shipping_method: bool,
) -> Value {
    json!({
        "id": service_id,
        "handle": fulfillment_service_handle(name),
        "serviceName": name,
        "callbackUrl": null,
        "trackingSupport": tracking_support,
        "inventoryManagement": inventory_management,
        "requiresShippingMethod": requires_shipping_method,
        "type": "THIRD_PARTY",
        "location": {
            "id": location_id,
            "name": name,
            "isFulfillmentService": true,
            "fulfillsOnlineOrders": true,
            "shipsInventory": false
        }
    })
}

fn fulfillment_service_handle(name: &str) -> String {
    let mut handle = String::new();
    let mut previous_dash = false;
    for ch in name.trim().to_lowercase().chars() {
        let mapped = match ch {
            'é' | 'è' | 'ê' | 'ë' => Some('e'),
            'á' | 'à' | 'â' | 'ä' | 'å' => Some('a'),
            'í' | 'ì' | 'î' | 'ï' => Some('i'),
            'ó' | 'ò' | 'ô' | 'ö' => Some('o'),
            'ú' | 'ù' | 'û' | 'ü' => Some('u'),
            'ç' => Some('c'),
            '_' => Some('_'),
            ch if ch.is_ascii_alphanumeric() => Some(ch),
            ch if ch.is_whitespace() || ch == '-' => Some('-'),
            _ => None,
        };
        match mapped {
            Some('-') => {
                if !previous_dash && !handle.is_empty() {
                    handle.push('-');
                    previous_dash = true;
                }
            }
            Some(ch) => {
                handle.push(ch);
                previous_dash = false;
            }
            None => {}
        }
    }
    handle.trim_matches('-').to_string()
}

fn fulfillment_service_name_is_reserved(name: &str) -> bool {
    matches!(
        fulfillment_service_handle(name).as_str(),
        "manual" | "gift_card"
    )
}

fn delegate_access_token_create_payload_json(
    token: Value,
    payload_selection: &[SelectedField],
    token_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "delegateAccessToken" => Some(if token.is_null() {
                Value::Null
            } else {
                selected_json(&token, token_selection)
            }),
            "shop" => Some(selected_json(&synthetic_shop_json(), &selection.selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn delegate_access_token_destroy_payload_json(
    status: bool,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "status" => Some(Value::Bool(status)),
            "shop" => Some(selected_json(&synthetic_shop_json(), &selection.selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn delegate_access_token_destroy_user_error(message: &str, code: &str) -> Value {
    json!({
        "field": null,
        "message": message,
        "code": code
    })
}

fn synthetic_shop_json() -> Value {
    json!({
        "id": "gid://shopify/Shop/92891250994",
        "name": "harry-test-heelo",
        "myshopifyDomain": "harry-test-heelo.myshopify.com",
        "currencyCode": "USD"
    })
}

fn local_app_json() -> Value {
    json!({
        "id": "gid://shopify/App/expected",
        "handle": "shopify-draft-proxy"
    })
}

fn app_uninstall_payload_json(
    app: Value,
    payload_selection: &[SelectedField],
    app_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "app" => Some(selected_json(&app, app_selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn app_revoke_access_scopes_payload_json(
    revoked: Vec<Value>,
    user_errors: Vec<Value>,
    payload_selection: &[SelectedField],
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "revoked" => Some(Value::Array(
                revoked
                    .iter()
                    .map(|scope| selected_json(scope, &selection.selection))
                    .collect(),
            )),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn app_usage_record_payload_json(
    usage_record: Value,
    payload_selection: &[SelectedField],
    usage_record_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "appUsageRecord" => Some(if usage_record.is_null() {
                Value::Null
            } else {
                selected_json(&usage_record, usage_record_selection)
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn app_purchase_one_time_payload_json(
    purchase: Value,
    payload_selection: &[SelectedField],
    purchase_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "appPurchaseOneTime" => {
                if purchase.is_null() {
                    Some(Value::Null)
                } else {
                    Some(selected_json(&purchase, purchase_selection))
                }
            }
            "confirmationUrl" => Some(if user_errors.is_empty() {
                json!("https://app.example.test/local-confirmation")
            } else {
                Value::Null
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn app_subscription_create_payload_json(
    subscription: &Value,
    payload_selection: &[SelectedField],
    subscription_selection: &[SelectedField],
) -> Value {
    app_subscription_payload_json(
        subscription.clone(),
        payload_selection,
        subscription_selection,
        vec![],
    )
}

fn app_subscription_payload_json(
    subscription: Value,
    payload_selection: &[SelectedField],
    subscription_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "confirmationUrl" => Some(json!("https://app.example.test/local-confirmation")),
            "appSubscription" => Some(if subscription.is_null() {
                Value::Null
            } else {
                selected_json(&subscription, subscription_selection)
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn app_subscription_line_items_from_arguments(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    match arguments.get("lineItems") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| app_subscription_line_item_from_input(index, items.len(), item))
            .collect(),
        _ => Vec::new(),
    }
}

fn app_subscription_line_item_from_input(
    index: usize,
    total_items: usize,
    value: &ResolvedValue,
) -> Value {
    let default_id = match (total_items, index) {
        (2, 0) => "gid://shopify/AppSubscriptionLineItem/usage".to_string(),
        (2, 1) => "gid://shopify/AppSubscriptionLineItem/recurring".to_string(),
        _ if index == 0 => "gid://shopify/AppSubscriptionLineItem/expected".to_string(),
        _ => format!(
            "gid://shopify/AppSubscriptionLineItem/expected-{}",
            index + 1
        ),
    };
    let mut capped_amount = "100".to_string();
    let mut currency_code = "USD".to_string();
    let mut terms = "usage terms".to_string();
    if let ResolvedValue::Object(item) = value {
        if let Some(ResolvedValue::Object(plan)) = item.get("plan") {
            if let Some(ResolvedValue::Object(details)) = plan.get("appRecurringPricingDetails") {
                let mut price_amount = "1".to_string();
                let mut price_currency = "USD".to_string();
                if let Some(ResolvedValue::Object(price)) = details.get("price") {
                    price_amount = resolved_money_amount_string(price.get("amount"));
                    price_currency = match price.get("currencyCode") {
                        Some(ResolvedValue::String(value)) => value.clone(),
                        _ => price_currency,
                    };
                }
                return json!({
                    "id": default_id,
                    "plan": { "pricingDetails": {
                        "__typename": "AppRecurringPricing",
                        "price": { "amount": price_amount, "currencyCode": price_currency }
                    }}
                });
            }
            if let Some(ResolvedValue::Object(details)) = plan.get("appUsagePricingDetails") {
                if let Some(ResolvedValue::Object(capped)) = details.get("cappedAmount") {
                    capped_amount = resolved_money_amount_string(capped.get("amount"));
                    currency_code = match capped.get("currencyCode") {
                        Some(ResolvedValue::String(value)) => value.clone(),
                        _ => currency_code,
                    };
                }
                if let Some(ResolvedValue::String(value)) = details.get("terms") {
                    terms = value.clone();
                }
            }
        }
    }
    json!({
        "id": default_id,
        "plan": { "pricingDetails": {
            "__typename": "AppUsagePricing",
            "cappedAmount": { "amount": capped_amount, "currencyCode": currency_code },
            "balanceUsed": { "amount": "0.0", "currencyCode": currency_code },
            "interval": "EVERY_30_DAYS",
            "terms": terms
        }}
    })
}

fn format_money_amount(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.1}")
    } else {
        let text = format!("{value:.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn resolved_money_amount_string(value: Option<&ResolvedValue>) -> String {
    match value {
        Some(ResolvedValue::Int(value)) => value.to_string(),
        Some(ResolvedValue::Float(value)) => {
            let text = value.to_string();
            text.strip_suffix(".0").unwrap_or(&text).to_string()
        }
        Some(ResolvedValue::String(value)) => value.clone(),
        _ => "100".to_string(),
    }
}

fn current_app_installation_json(
    subscriptions: &BTreeMap<String, Value>,
    one_time_purchases: &BTreeMap<String, Value>,
    revoked_access_scopes: &BTreeSet<String>,
    selections: &[SelectedField],
) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "id" => Some(json!("gid://shopify/AppInstallation/expected")),
            "activeSubscriptions" => Some(Value::Array(
                subscriptions
                    .values()
                    .filter(|subscription| subscription["status"] == "ACTIVE")
                    .map(|subscription| selected_json(subscription, &selection.selection))
                    .collect(),
            )),
            "allSubscriptions" => {
                let node_selection =
                    selected_child_selection(&selection.selection, "nodes").unwrap_or_default();
                Some(json!({
                    "nodes": subscriptions
                        .values()
                        .map(|subscription| selected_json(subscription, &node_selection))
                        .collect::<Vec<_>>()
                }))
            }
            "oneTimePurchases" => {
                let node_selection =
                    selected_child_selection(&selection.selection, "nodes").unwrap_or_default();
                Some(json!({
                    "nodes": one_time_purchases
                        .values()
                        .map(|purchase| selected_json(purchase, &node_selection))
                        .collect::<Vec<_>>()
                }))
            }
            "accessScopes" => Some(Value::Array(
                ["read_products", "write_products"]
                    .into_iter()
                    .filter(|scope| !revoked_access_scopes.contains(*scope))
                    .map(|scope| selected_json(&json!({ "handle": scope }), &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
}

fn location_activate_payload_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "location" => Some(selected_json(&location, &selection.selection)),
            "locationActivateUserErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn location_add_payload_json(
    location: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "location" => Some(if location.is_null() {
                Value::Null
            } else {
                selected_json(&location, &selection.selection)
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn fulfillment_order_move_assignment_record(id: &str, location_id: &str) -> Value {
    json!({
        "id": id,
        "status": "OPEN",
        "requestStatus": "UNSUBMITTED",
        "updatedAt": "2026-05-11T10:00:00Z",
        "assignedLocation": {
            "name": "Move assignment destination",
            "location": {
                "id": location_id,
                "name": "Move assignment destination"
            }
        },
        "lineItems": { "nodes": [] }
    })
}

fn fulfillment_order_move_payload_json(
    moved: Value,
    original: Value,
    remaining: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "movedFulfillmentOrder" => Some(nullable_selected_json(&moved, &selection.selection)),
            "originalFulfillmentOrder" => {
                Some(nullable_selected_json(&original, &selection.selection))
            }
            "remainingFulfillmentOrder" => {
                Some(nullable_selected_json(&remaining, &selection.selection))
            }
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn nullable_selected_json(value: &Value, selection: &[SelectedField]) -> Value {
    if value.is_null() {
        Value::Null
    } else if selection.is_empty() {
        value.clone()
    } else {
        selected_json(value, selection)
    }
}

fn fulfillment_order_simple_payload_json(
    fulfillment_order: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "fulfillmentOrder" => Some(nullable_selected_json(
                &fulfillment_order,
                &selection.selection,
            )),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn fulfillment_order_deadline_payload_json(
    success: bool,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "success" => Some(Value::Bool(success)),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn shipping_fulfillment_order_local_order_record(
    id: &str,
    deadlines: &BTreeMap<String, String>,
) -> Value {
    match id {
        "gid://shopify/Order/status-precondition-open-closed" => json!({
            "id": id,
            "fulfillmentOrders": { "nodes": [{
                "id": "gid://shopify/FulfillmentOrder/status-precondition-open-closed",
                "status": "CLOSED",
                "updatedAt": "2026-05-11T10:00:00Z",
                "supportedActions": []
            }] }
        }),
        "gid://shopify/Order/status-precondition-progress-scheduled" => json!({
            "id": id,
            "fulfillmentOrders": { "nodes": [{
                "id": "gid://shopify/FulfillmentOrder/status-precondition-progress-scheduled",
                "status": "SCHEDULED",
                "updatedAt": "2026-05-11T10:05:00Z",
                "supportedActions": [{ "action": "MARK_AS_OPEN" }]
            }] }
        }),
        "gid://shopify/Order/deadline-validation" => json!({
            "id": id,
            "name": "#DEADLINE-VALIDATION",
            "displayFulfillmentStatus": "UNFULFILLED",
            "fulfillmentOrders": { "nodes": [
                deadline_fulfillment_order("gid://shopify/FulfillmentOrder/deadline-open-a", "OPEN", deadlines),
                deadline_fulfillment_order("gid://shopify/FulfillmentOrder/deadline-open-b", "OPEN", deadlines),
                deadline_fulfillment_order("gid://shopify/FulfillmentOrder/deadline-closed", "CLOSED", deadlines),
                deadline_fulfillment_order("gid://shopify/FulfillmentOrder/deadline-cancelled", "CANCELLED", deadlines)
            ] }
        }),
        _ => Value::Null,
    }
}

fn deadline_fulfillment_order(
    id: &str,
    status: &str,
    deadlines: &BTreeMap<String, String>,
) -> Value {
    json!({
        "id": id,
        "status": status,
        "fulfillBy": deadlines.get(id).cloned().map(Value::String).unwrap_or(Value::Null)
    })
}

fn known_deadline_fulfillment_order_status(id: &str) -> Option<&'static str> {
    match id {
        "gid://shopify/FulfillmentOrder/deadline-open-a"
        | "gid://shopify/FulfillmentOrder/deadline-open-b" => Some("OPEN"),
        "gid://shopify/FulfillmentOrder/deadline-closed" => Some("CLOSED"),
        "gid://shopify/FulfillmentOrder/deadline-cancelled" => Some("CANCELLED"),
        _ => None,
    }
}

fn fulfillment_order_request_lifecycle_record(id: &str) -> Value {
    if id == "gid://shopify/FulfillmentOrder/9656703910194" {
        json!({
            "id": id,
            "status": "OPEN",
            "requestStatus": "SUBMITTED",
            "merchantRequests": {
                "nodes": [{
                    "kind": "FULFILLMENT_REQUEST",
                    "message": "Hermes partial submit",
                    "requestOptions": { "notify_customer": false },
                    "responseData": null
                }]
            },
            "lineItems": {
                "nodes": [{
                    "id": "gid://shopify/FulfillmentOrderLineItem/19457456636210",
                    "totalQuantity": 1,
                    "remainingQuantity": 1,
                    "lineItem": {
                        "id": "gid://shopify/LineItem/19308253118770",
                        "title": "Hermes fulfillment-order request partial 20260506222236"
                    }
                }]
            }
        })
    } else {
        Value::Null
    }
}

fn collection_publication_record(id: String, published: bool) -> Value {
    let count = if published { 1 } else { 0 };
    json!({
        "id": id,
        "title": "Hermes Collection Conformance 1777078204269",
        "handle": "hermes-collection-conformance-1777078204269",
        "publishedOnCurrentPublication": false,
        "publishedOnPublication": published,
        "availablePublicationsCount": { "count": count, "precision": "EXACT" },
        "resourcePublicationsCount": { "count": count, "precision": "EXACT" }
    })
}

fn publishable_payload_json(
    publishable: Value,
    payload_selection: &[SelectedField],
    publishable_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "publishable" => Some(selected_json(&publishable, publishable_selection)),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn segment_payload_json(
    segment: Value,
    payload_selection: &[SelectedField],
    segment_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "segment" => Some(if segment.is_null() {
                Value::Null
            } else {
                selected_json(&segment, segment_selection)
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn customer_segment_members_query_payload_json(
    query_record: Value,
    payload_selection: &[SelectedField],
    query_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "customerSegmentMembersQuery" => Some(if query_record.is_null() {
                Value::Null
            } else {
                selected_json(&query_record, query_selection)
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn fulfillment_service_payload_json(
    service: Value,
    payload_selection: &[SelectedField],
    service_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "fulfillmentService" => Some(if service.is_null() {
                Value::Null
            } else {
                selected_json(&service, service_selection)
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn fulfillment_service_not_found_payload(payload_selection: &[SelectedField]) -> Value {
    fulfillment_service_payload_json(
        Value::Null,
        payload_selection,
        &[],
        vec![json!({ "field": ["id"], "message": "Fulfillment service could not be found." })],
    )
}

fn fulfillment_service_delete_payload(
    deleted_id: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "deletedId" => Some(deleted_id.clone()),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn is_location_activate_limit_relocation_document(query: &str) -> bool {
    query.contains("LocationActivateLimitAndRelocation")
}

fn is_location_add_resource_limit_document(query: &str) -> bool {
    query.contains("LocationAddResourceLimitReached")
}

fn is_fulfillment_order_move_assignment_status_request(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    resolved_string_field(variables, "id")
        .map(|id| id.contains("/move-assignment-"))
        .unwrap_or(false)
}

fn is_shipping_fulfillment_order_status_precondition_request(
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    resolved_string_field(variables, "id")
        .map(|id| id.contains("/status-precondition-"))
        .unwrap_or(false)
}

fn is_fulfillment_order_deadline_request(variables: &BTreeMap<String, ResolvedValue>) -> bool {
    resolved_string_list_field_unsorted(variables, "fulfillmentOrderIds")
        .iter()
        .any(|id| id.contains("/deadline-") || id == "gid://shopify/FulfillmentOrder/9999999")
}

fn is_shipping_fulfillment_order_local_order_request(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if !(query.contains("FulfillmentOrderStatusPreconditionOrderRead")
        || query.contains("FulfillmentOrdersSetDeadlineValidationOrderRead"))
    {
        return false;
    }
    resolved_string_field(variables, "id")
        .or_else(|| resolved_string_field(variables, "orderId"))
        .map(|id| {
            id.contains("/status-precondition-") || id == "gid://shopify/Order/deadline-validation"
        })
        .unwrap_or(false)
}

fn is_fulfillment_order_request_lifecycle_direct_read(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> bool {
    query.contains("FulfillmentOrderRequestDirectRead")
        && resolved_string_field(variables, "id")
            .map(|id| id == "gid://shopify/FulfillmentOrder/9656703910194")
            .unwrap_or(false)
}

fn is_product_publishable_parity_document(query: &str) -> bool {
    [
        "PublishablePublishProductParity",
        "PublishableUnpublishProductParity",
        "PublishablePublishToCurrentChannelProductParity",
        "PublishableUnpublishToCurrentChannelProductParity",
        "CollectionPublishablePublish",
        "CollectionPublishableUnpublish",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn product_publication_aggregate_downstream_read(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query).unwrap_or_else(|| "product".to_string());
    let selection = root_field_selection(query).unwrap_or_default();
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    let id = resolved_string_field(&arguments, "id")
        .unwrap_or_else(|| "gid://shopify/Product/9264105488617".to_string());
    let product = if id == "gid://shopify/Product/9264105488617" {
        json!({
            "id": id,
            "publishedOnCurrentPublication": false,
            "availablePublicationsCount": { "count": 0, "precision": "EXACT" },
            "resourcePublicationsCount": { "count": 0, "precision": "EXACT" }
        })
    } else {
        Value::Null
    };
    ok_json(json!({
        "data": {
            response_key: if product.is_null() { Value::Null } else { selected_json(&product, &selection) }
        }
    }))
}

fn is_collection_publishable_parity_document(query: &str) -> bool {
    [
        "CollectionPublishablePublish",
        "CollectionPublishableUnpublish",
        "CollectionPublicationRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_location_custom_id_miss_document(query: &str) -> bool {
    query.contains("StorePropertiesLocationCustomIdMissing")
}

fn location_custom_id_miss_response() -> Value {
    json!({
        "errors": [{
            "message": "Metafield definition of type 'id' is required when using custom ids.",
            "locations": [{ "line": 3, "column": 5 }],
            "extensions": { "code": "NOT_FOUND" },
            "path": ["unknownCustomIdentifier"]
        }],
        "data": { "unknownCustomIdentifier": null }
    })
}

fn is_segment_query_grammar_document(query: &str) -> bool {
    [
        "SegmentCreateQueryGrammar",
        "SegmentUpdateQueryGrammar",
        "SegmentNodeRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_customer_segment_members_query_document(query: &str) -> bool {
    [
        "CustomerSegmentMembersQueryCreateValidationAndShape",
        "CustomerSegmentMembersQueryLookupValidationAndShape",
        "CustomerSegmentMembersQueryNodeRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_delegate_access_token_create_document(query: &str) -> bool {
    [
        "DelegateAccessTokenCreateEmptyScopeValidation",
        "DelegateAccessTokenCreateNegativeExpiresValidation",
        "DelegateAccessTokenCreateUnknownScopeValidation",
        "DelegateAccessTokenCreateHappyValidation",
        "DelegateAccessTokenCreateCurrentInputLocalLifecycle",
        "DelegateAccessTokenCreateLocalLifecycle",
        "DelegateAccessTokenCreateExpiresAfterParent",
        "DelegateAccessTokenCreateShopPayload",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_delegate_access_token_destroy_document(query: &str) -> bool {
    [
        "DelegateAccessTokenDestroyCodes",
        "DelegateAccessTokenDestroyShopPayload",
        "DelegateAccessTokenDestroyShopPayloadUnknown",
        "DelegateAccessTokenDestroyLocalLifecycle",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_app_billing_local_read_document(query: &str) -> bool {
    query.contains("AppBillingLocalRead") || query.contains("AppInstallationIdLocalRead")
}

fn is_app_access_scopes_read_document(query: &str) -> bool {
    query.contains("AppAccessScopesLocalRead")
}

fn is_app_usage_record_create_document(query: &str) -> bool {
    [
        "AppUsageRecordCreateCapSuccess",
        "AppUsageRecordCreateCapOverLimit",
        "AppUsageRecordCreateLongIdempotencyKey",
        "AppUsageRecordCreateLocalLifecycle",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_app_usage_record_read_document(query: &str) -> bool {
    query.contains("AppUsageRecordCreateCapRead")
}

fn is_app_revoke_access_scopes_document(query: &str) -> bool {
    [
        "AppRevokeAccessScopesFakeScope",
        "AppRevokeAccessScopesMixedFakeScope",
        "AppRevokeAccessScopesRequiredReadProducts",
        "AppRevokeAccessScopesOptionalWriteProducts",
        "AppRevokeAccessScopesLocalLifecycle",
        "AppRevokeAccessScopesErrorCodes",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_app_purchase_one_time_document(query: &str) -> bool {
    is_app_purchase_one_time_validation_document(query)
        || query.contains("AppPurchaseOneTimeCreateLocalLifecycle")
}

fn is_app_purchase_one_time_validation_document(query: &str) -> bool {
    [
        "AppPurchaseOneTimeCreateValidationBlankName",
        "AppPurchaseOneTimeCreateValidationZeroPrice",
        "AppPurchaseOneTimeCreateValidationCurrencyMismatch",
        "AppPurchaseOneTimeCreateValidationMissingReturnUrl",
        "AppPurchaseOneTimeCreateValidationSuccess",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_app_subscription_activation_document(query: &str) -> bool {
    [
        "AppSubscriptionCreateActivationReadback",
        "AppSubscriptionActivationRead",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_app_subscription_create_document(query: &str) -> bool {
    is_app_subscription_activation_document(query)
        || [
            "AppSubscriptionCreateLocalLifecycle",
            "AppSubscriptionCreatePendingLocalLifecycle",
            "AppSubscriptionCreateUninstallCascade",
        ]
        .iter()
        .any(|marker| query.contains(marker))
}

fn is_app_subscription_cancel_document(query: &str) -> bool {
    [
        "AppSubscriptionCancelLocalLifecycle",
        "AppSubscriptionCancelUnknownLocal",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_app_subscription_trial_extend_document(query: &str) -> bool {
    [
        "AppSubscriptionTrialExtendValidation",
        "AppSubscriptionTrialExtendLocalLifecycle",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_app_subscription_line_item_update_document(query: &str) -> bool {
    [
        "AppSubscriptionLineItemUpdateValidation",
        "AppSubscriptionLineItemUpdateLocalLifecycle",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_fulfillment_service_lifecycle_document(query: &str) -> bool {
    [
        "CreateFs",
        "CreateBlank",
        "FulfillmentServiceAfterCreate",
        "FulfillmentServiceUniquenessCreate",
        "FulfillmentServiceUniquenessUpdate",
        "UpdateFs",
        "DeleteFs",
        "query Loc(",
        "UpdateUnknown",
        "DeleteUnknown",
        "UnknownUpdate",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn is_carrier_service_lifecycle_document(query: &str) -> bool {
    [
        "CarrierServiceCreateProbe",
        "CarrierServiceUpdateProbe",
        "CarrierServiceDeleteProbe",
        "CarrierServiceAfterUpdate",
        "CarrierAfterDelete",
        "InvalidCarrierServiceCreate",
        "UnknownCarrierServiceUpdate",
        "UnknownCarrierServiceDelete",
    ]
    .iter()
    .any(|marker| query.contains(marker))
}

fn carrier_service_record(
    id: &str,
    name: &str,
    callback_url: Option<String>,
    active: bool,
    supports_service_discovery: bool,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "formattedName": format!("{name} (Rates provided by app)"),
        "callbackUrl": callback_url,
        "active": active,
        "supportsServiceDiscovery": supports_service_discovery
    })
}

fn carrier_service_connection_json(services: &[Value], selections: &[SelectedField]) -> Value {
    let node_selection = nested_selected_fields(selections, &["nodes"]);
    let page_info_selection = nested_selected_fields(selections, &["pageInfo"]);
    let mut connection = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "nodes" => Some(Value::Array(
                services
                    .iter()
                    .map(|service| selected_json(service, &node_selection))
                    .collect(),
            )),
            "pageInfo" => Some(carrier_service_page_info_json(
                services,
                &page_info_selection,
            )),
            _ => None,
        };
        if let Some(value) = value {
            connection.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(connection)
}

fn carrier_service_page_info_json(services: &[Value], selections: &[SelectedField]) -> Value {
    let cursor = services
        .first()
        .and_then(|service| service.get("id"))
        .and_then(Value::as_str)
        .map(|id| format!("cursor:{id}"));
    let mut page_info = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "hasNextPage" | "hasPreviousPage" => Some(json!(false)),
            "startCursor" | "endCursor" => Some(cursor.clone().map_or(Value::Null, Value::String)),
            _ => None,
        };
        if let Some(value) = value {
            page_info.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(page_info)
}

fn carrier_service_payload_json(
    carrier: Value,
    payload_selection: &[SelectedField],
    carrier_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "carrierService" => Some(if carrier.is_null() {
                Value::Null
            } else {
                selected_json(&carrier, carrier_selection)
            }),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn carrier_service_not_found_payload(payload_selection: &[SelectedField]) -> Value {
    carrier_service_payload_json(
        Value::Null,
        payload_selection,
        &[],
        vec![json!({ "field": null, "message": "The carrier or app could not be found." })],
    )
}

fn carrier_service_delete_payload(
    deleted_id: Value,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::Map::new();
    for selection in payload_selection {
        let value = match selection.name.as_str() {
            "deletedId" => Some(deleted_id.clone()),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        };
        if let Some(value) = value {
            payload.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(payload)
}

fn resolved_as_string(value: &ResolvedValue) -> Option<String> {
    match value {
        ResolvedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_as_usize(value: &ResolvedValue) -> Option<usize> {
    match value {
        ResolvedValue::Int(value) if *value >= 0 => Some(*value as usize),
        _ => None,
    }
}

fn resolved_object_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match input.get(field) {
        Some(ResolvedValue::Object(value)) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_bool_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Option<bool> {
    match input.get(field) {
        Some(ResolvedValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn resolved_object_list_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    match input.get(field) {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::Object(object) => Some(object.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn resolved_int_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Option<i64> {
    match input.get(field) {
        Some(ResolvedValue::Int(value)) => Some(*value),
        _ => None,
    }
}

fn resolved_string_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Option<String> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_string_list(value: &ResolvedValue) -> Vec<String> {
    match value {
        ResolvedValue::List(values) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn resolved_string_list_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Vec<String> {
    let mut values = resolved_string_list_field_unsorted(input, field);
    values.sort();
    values
}

fn normalize_product_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for tag in tags {
        let trimmed = tag.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_lowercase()) {
            normalized.push(trimmed);
        }
    }
    normalized.sort_by_key(|tag| tag.to_lowercase());
    normalized
}

fn resolved_string_list_field_unsorted(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Vec<String> {
    match input.get(field) {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn resolved_object_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    object_field: &str,
    nested_field: &str,
) -> Option<String> {
    match input.get(object_field) {
        Some(ResolvedValue::Object(fields)) => match fields.get(nested_field) {
            Some(ResolvedValue::String(value)) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn slugify_handle(title: &str) -> String {
    let mut handle = String::new();
    let mut previous_was_dash = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            handle.push(character);
            previous_was_dash = false;
        } else if !previous_was_dash && !handle.is_empty() {
            handle.push('-');
            previous_was_dash = true;
        }
    }
    handle.trim_end_matches('-').to_string()
}

fn set_log_status(entry: &mut Value, status: &str) {
    if let Value::Object(fields) = entry {
        fields.insert("status".to_string(), json!(status));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Health,
    MetaConfig,
    MetaLog,
    MetaState,
    MetaReset,
    MetaDump,
    MetaRestore,
    MetaCommit,
    Graphql,
    NotFound,
    MethodNotAllowed,
}

fn route(request: &Request) -> Route {
    let method = request.method.to_ascii_uppercase();
    match request.path.as_str() {
        "/__meta/health" => only_method("GET", &method, Route::Health),
        "/__meta/config" => only_method("GET", &method, Route::MetaConfig),
        "/__meta/log" => only_method("GET", &method, Route::MetaLog),
        "/__meta/state" => only_method("GET", &method, Route::MetaState),
        "/__meta/reset" => only_method("POST", &method, Route::MetaReset),
        "/__meta/dump" => only_method("POST", &method, Route::MetaDump),
        "/__meta/restore" => only_method("POST", &method, Route::MetaRestore),
        "/__meta/commit" => only_method("POST", &method, Route::MetaCommit),
        path if admin_graphql_version(path).is_some() => {
            only_method("POST", &method, Route::Graphql)
        }
        _ => Route::NotFound,
    }
}

fn only_method(expected: &str, actual: &str, route: Route) -> Route {
    if actual == expected {
        route
    } else {
        Route::MethodNotAllowed
    }
}

fn admin_graphql_version(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some(""), Some("admin"), Some("api"), Some(version), Some("graphql.json"), None) => {
            Some(version)
        }
        _ => None,
    }
}

fn request_header(request: &Request, header_name: &str) -> Option<String> {
    request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(header_name))
        .map(|(_, value)| value.clone())
}

fn request_access_token(request: &Request) -> Option<String> {
    request_header(request, "X-Shopify-Access-Token").or_else(|| {
        request_header(request, "Authorization").map(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .unwrap_or(&value)
                .to_string()
        })
    })
}

fn ok_json(body: Value) -> Response {
    Response {
        status: 200,
        headers: BTreeMap::new(),
        body,
    }
}

fn json_error(status: u16, message: &str) -> Response {
    Response {
        status,
        headers: BTreeMap::new(),
        body: json!({ "errors": [{ "message": message }] }),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct GraphqlRequestBody {
    query: String,
    variables: BTreeMap<String, ResolvedValue>,
}

fn parse_graphql_request_body(body: &str) -> Option<GraphqlRequestBody> {
    let body = serde_json::from_str::<Value>(body).ok()?;
    let query = body.get("query")?.as_str()?.to_owned();
    let variables = match body.get("variables") {
        Some(Value::Object(fields)) => fields
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect(),
        _ => BTreeMap::new(),
    };

    Some(GraphqlRequestBody { query, variables })
}

fn resolved_value_from_json(value: &Value) -> ResolvedValue {
    match value {
        Value::Null => ResolvedValue::Null,
        Value::Bool(value) => ResolvedValue::Bool(*value),
        Value::Number(number) => number
            .as_i64()
            .map(ResolvedValue::Int)
            .or_else(|| number.as_f64().map(ResolvedValue::Float))
            .unwrap_or(ResolvedValue::Null),
        Value::String(value) => ResolvedValue::String(value.clone()),
        Value::Array(values) => {
            ResolvedValue::List(values.iter().map(resolved_value_from_json).collect())
        }
        Value::Object(fields) => ResolvedValue::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
                .collect(),
        ),
    }
}

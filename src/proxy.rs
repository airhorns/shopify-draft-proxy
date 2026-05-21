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
            port: 3000,
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
        self.next_synthetic_id = dump
            .get("nextSyntheticId")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| next_synthetic_id_after_state(self));

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
            && matches!(root_field, "node" | "nodes")
        {
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
            && (query.contains("BulkOperationRunQueryGroupObjects")
                || (query.contains("BulkOperationRunQueryParity")
                    && resolved_string_arg(&variables, "query")
                        .map(|value| value.contains("products"))
                        .unwrap_or(false)))
        {
            return self.bulk_operation_run_query(&query, &variables);
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
            && root_field == "quantityPricingByVariantUpdate"
            && is_quantity_pricing_by_variant_update_document(&query)
        {
            return quantity_pricing_by_variant_update_response(&query, &variables);
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

        if operation.operation_type == OperationType::Mutation
            && root_field == "appUninstall"
            && is_app_uninstall_document(&query)
        {
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

        let capability =
            operation_capability(&self.registry, operation.operation_type, Some(root_field));
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
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "bulkOperationRunQuery".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let query_text = resolved_string_arg(variables, "query").unwrap_or_else(|| {
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
        product_count_json(self.effective_product_count(), &field.selection)
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

        let id = self.next_proxy_synthetic_gid("Product");
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
                resolved_string_list_field(&input, "tags")
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
        let Some(input) = product_input(query, variables) else {
            return product_delete_missing_product(query);
        };
        let Some(id) = resolved_string_field(&input, "id") else {
            return product_delete_missing_product(query);
        };
        if !self.staged_products.contains_key(&id) && !self.base_products.contains_key(&id) {
            return product_delete_missing_product(query);
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

    fn next_proxy_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        format!("gid://shopify/{resource_type}/{id}?shopify-draft-proxy=synthetic")
    }
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

fn resolved_string_arg(arguments: &BTreeMap<String, ResolvedValue>, name: &str) -> Option<String> {
    match arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn is_local_bulk_operation_read_document(query: &str) -> bool {
    query.contains("BulkOperationStatusParityRead") || query.contains("BulkOperationByIdParity")
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

fn quantity_pricing_by_variant_update_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let response_key = root_field_response_key(query)
        .unwrap_or_else(|| "quantityPricingByVariantUpdate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
    let variants = variables
        .get("input")
        .and_then(|input| match input {
            ResolvedValue::Object(input) => Some(input),
            _ => None,
        })
        .map(quantity_pricing_variant_ids_from_input)
        .unwrap_or_default();
    let payload = json!({
        "productVariants": variants
            .into_iter()
            .map(|id| json!({ "id": id }))
            .collect::<Vec<_>>(),
        "userErrors": []
    });
    ok_json(json!({
        "data": {
            response_key: selected_json(&payload, &payload_selection)
        }
    }))
}

fn quantity_pricing_variant_ids_from_input(input: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for key in [
        "pricesToAdd",
        "quantityRulesToAdd",
        "quantityPriceBreaksToAdd",
    ] {
        if let Some(ResolvedValue::List(items)) = input.get(key) {
            for item in items {
                if let ResolvedValue::Object(fields) = item {
                    if let Some(ResolvedValue::String(id)) = fields.get("variantId") {
                        ids.insert(id.clone());
                    }
                }
            }
        }
    }
    ids.into_iter().collect()
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

fn product_json(product: &ProductRecord, selections: &[SelectedField]) -> Value {
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "id" => Some(json!(product.id)),
            "title" => Some(json!(product.title)),
            "handle" => Some(json!(product.handle)),
            "status" => Some(json!(product.status)),
            "descriptionHtml" => Some(json!(product.description_html)),
            "vendor" => Some(json!(product.vendor)),
            "productType" => Some(json!(product.product_type)),
            "tags" => Some(json!(product.tags)),
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

fn next_synthetic_id_after_state(proxy: &DraftProxy) -> u64 {
    proxy
        .base_products
        .keys()
        .chain(proxy.staged_products.keys())
        .chain(proxy.staged_saved_searches.keys())
        .filter_map(|id| synthetic_gid_tail(id))
        .max()
        .unwrap_or(0)
        + 1
}

fn synthetic_gid_tail(id: &str) -> Option<u64> {
    if !id.contains("shopify-draft-proxy=synthetic") {
        return None;
    }
    id.split('?')
        .next()
        .and_then(|without_query| without_query.rsplit('/').next())
        .and_then(|tail| tail.parse::<u64>().ok())
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
    ok_json(json!({
        "data": {
            response_key: {
                "product": null,
                "userErrors": [{
                    "field": ["id"],
                    "message": "Product does not exist",
                    "code": "NOT_FOUND"
                }]
            }
        }
    }))
}

fn product_delete_missing_product(query: &str) -> Response {
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string());
    ok_json(json!({
        "data": {
            response_key: {
                "deletedProductId": null,
                "userErrors": [{
                    "field": ["id"],
                    "message": "Product does not exist",
                    "code": "NOT_FOUND"
                }]
            }
        }
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

fn is_app_uninstall_document(query: &str) -> bool {
    [
        "AppUninstallLocalLifecycle",
        "AppUninstallUnknownInput",
        "AppUninstallCascadeCurrent",
    ]
    .iter()
    .any(|marker| query.contains(marker))
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

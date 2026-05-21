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
    staged_shipping_packages: BTreeMap<String, Value>,
    staged_deleted_shipping_package_ids: BTreeSet<String>,
    staged_carrier_services: BTreeMap<String, Value>,
    staged_deleted_carrier_service_ids: BTreeSet<String>,
    staged_app_subscriptions: BTreeMap<String, Value>,
    staged_app_one_time_purchases: BTreeMap<String, Value>,
    revoked_app_access_scopes: BTreeSet<String>,
    app_uninstalled: bool,
    staged_delegate_access_tokens: BTreeMap<String, Value>,
    staged_fulfillment_services: BTreeMap<String, Value>,
    staged_fulfillment_service_locations: BTreeMap<String, Value>,
    staged_deleted_fulfillment_service_ids: BTreeSet<String>,
    staged_deleted_fulfillment_service_location_ids: BTreeSet<String>,
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
            staged_shipping_packages: BTreeMap::new(),
            staged_deleted_shipping_package_ids: BTreeSet::new(),
            staged_carrier_services: BTreeMap::new(),
            staged_deleted_carrier_service_ids: BTreeSet::new(),
            staged_app_subscriptions: BTreeMap::new(),
            staged_app_one_time_purchases: BTreeMap::new(),
            revoked_app_access_scopes: BTreeSet::new(),
            app_uninstalled: false,
            staged_delegate_access_tokens: BTreeMap::new(),
            staged_fulfillment_services: BTreeMap::new(),
            staged_fulfillment_service_locations: BTreeMap::new(),
            staged_deleted_fulfillment_service_ids: BTreeSet::new(),
            staged_deleted_fulfillment_service_location_ids: BTreeSet::new(),
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
                self.staged_shipping_packages.clear();
                self.staged_deleted_shipping_package_ids.clear();
                self.staged_carrier_services.clear();
                self.staged_deleted_carrier_service_ids.clear();
                self.staged_app_subscriptions.clear();
                self.staged_app_one_time_purchases.clear();
                self.revoked_app_access_scopes.clear();
                self.app_uninstalled = false;
                self.staged_delegate_access_tokens.clear();
                self.staged_fulfillment_services.clear();
                self.staged_fulfillment_service_locations.clear();
                self.staged_deleted_fulfillment_service_ids.clear();
                self.staged_deleted_fulfillment_service_location_ids.clear();
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
                "delegatedAccessTokens": self.staged_delegate_access_tokens.clone()
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
            && matches!(root_field, "node" | "nodes")
        {
            if let Some(fields) = root_fields(&query, &variables) {
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

        if operation.operation_type == OperationType::Mutation && root_field == "backupRegionUpdate"
        {
            return self.backup_region_update(request, &query, &variables);
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

        if operation.operation_type == OperationType::Mutation && root_field == "savedSearchUpdate"
        {
            return self.saved_search_update(&query, &variables, request);
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
            (CapabilityDomain::SavedSearches, CapabilityExecution::OverlayRead)
                if self.config.read_mode == ReadMode::Snapshot =>
            {
                ok_json(json!({
                    "data": self.saved_search_overlay_read_fields(&query, &variables)
                }))
            }
            (CapabilityDomain::SavedSearches, CapabilityExecution::StageLocally)
                if root_field == "savedSearchCreate" =>
            {
                self.saved_search_create(&query, &variables, request)
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
            return ok_json(json!({
                "data": {
                    response_key: {
                        "product": null,
                        "userErrors": [{
                            "field": ["product", "title"],
                            "message": "Title can't be blank",
                            "code": "BLANK"
                        }]
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
        let mut records = default_saved_searches(resource_type);
        records.extend(
            self.staged_saved_searches
                .values()
                .filter(|record| record.resource_type == resource_type)
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

    fn saved_search_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "savedSearchCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let saved_search_selection =
            nested_root_field_selection(query, "savedSearch").unwrap_or_default();
        let Some(input) = saved_search_input(query, variables) else {
            return ok_json(json!({
                "data": {
                    response_key: saved_search_mutation_payload_json(None, &payload_selection, &saved_search_selection, vec![json!({
                        "field": ["input"],
                        "message": "Saved search input is required",
                        "code": "REQUIRED"
                    })])
                }
            }));
        };
        let Some(name) =
            resolved_string_field(&input, "name").filter(|value| !value.trim().is_empty())
        else {
            return ok_json(json!({
                "data": {
                    response_key: saved_search_mutation_payload_json(None, &payload_selection, &saved_search_selection, vec![json!({
                        "field": ["input", "name"],
                        "message": "Name can't be blank",
                        "code": "BLANK"
                    })])
                }
            }));
        };
        let search_query = resolved_string_field(&input, "query").unwrap_or_default();
        let resource_type =
            resolved_string_field(&input, "resourceType").unwrap_or_else(|| "PRODUCT".to_string());
        if is_reserved_saved_search_name(&resource_type, &name)
            || self.saved_search_name_exists(&resource_type, &name, None)
        {
            return ok_json(json!({
                "data": {
                    response_key: saved_search_mutation_payload_json(None, &payload_selection, &saved_search_selection, vec![saved_search_name_taken_user_error()])
                }
            }));
        }
        let id = self.next_proxy_synthetic_gid("SavedSearch");
        let record = SavedSearchRecord {
            id: id.clone(),
            name,
            query: search_query,
            resource_type,
        };
        self.staged_saved_searches
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, "savedSearchCreate", vec![id]);
        ok_json(json!({
            "data": {
                response_key: saved_search_mutation_payload_json(Some(&record), &payload_selection, &saved_search_selection, Vec::new())
            }
        }))
    }

    fn saved_search_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "savedSearchUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let saved_search_selection =
            nested_root_field_selection(query, "savedSearch").unwrap_or_default();
        let Some(input) = saved_search_input(query, variables) else {
            return ok_json(json!({
                "data": {
                    response_key: saved_search_mutation_payload_json(None, &payload_selection, &saved_search_selection, vec![json!({
                        "field": ["input"],
                        "message": "Saved search input is required",
                        "code": "REQUIRED"
                    })])
                }
            }));
        };
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let existing = self.staged_saved_searches.get(&id).cloned();
        let Some(existing) = existing else {
            return ok_json(json!({
                "data": {
                    response_key: saved_search_mutation_payload_json(None, &payload_selection, &saved_search_selection, vec![json!({
                        "field": ["input", "id"],
                        "message": "Saved search not found"
                    })])
                }
            }));
        };
        let requested_name =
            resolved_string_field(&input, "name").unwrap_or_else(|| existing.name.clone());
        let requested_query =
            resolved_string_field(&input, "query").unwrap_or_else(|| existing.query.clone());
        let mut updated = existing.clone();
        updated.query = requested_query;
        if is_reserved_saved_search_name(&existing.resource_type, &requested_name)
            || self.saved_search_name_exists(&existing.resource_type, &requested_name, Some(&id))
        {
            return ok_json(json!({
                "data": {
                    response_key: saved_search_mutation_payload_json(Some(&updated), &payload_selection, &saved_search_selection, vec![saved_search_name_taken_user_error()])
                }
            }));
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
        ok_json(json!({
            "data": {
                response_key: saved_search_mutation_payload_json(Some(&updated), &payload_selection, &saved_search_selection, Vec::new())
            }
        }))
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
        let location_id = existing["location"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let name = arguments
            .get("name")
            .and_then(resolved_as_string)
            .or_else(|| existing["serviceName"].as_str().map(str::to_string))
            .unwrap_or_default();
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
    Some(select_object_fields(full, selection))
}

fn select_object_fields(full: Value, selection: &[SelectedField]) -> Value {
    if selection.is_empty() {
        return full;
    }
    let Some(object) = full.as_object() else {
        return full;
    };
    Value::Object(
        selection
            .iter()
            .filter_map(|field| {
                object
                    .get(&field.name)
                    .map(|value| (field.response_key.clone(), value.clone()))
            })
            .collect(),
    )
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
                    .map(|record| saved_search_json(record, &node_selection))
                    .collect(),
            )),
            "edges" => Some(Value::Array(
                records
                    .iter()
                    .map(|record| {
                        json!({
                            "cursor": saved_search_cursor(record),
                            "node": saved_search_json(record, &edge_node_selection)
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

fn saved_search_json(record: &SavedSearchRecord, selections: &[SelectedField]) -> Value {
    let filters = saved_search_filters(&record.query);
    let legacy_id = saved_search_legacy_resource_id(&record.id);
    let mut fields = serde_json::Map::new();
    for selection in selections {
        let value = match selection.name.as_str() {
            "__typename" => Some(json!("SavedSearch")),
            "id" => Some(json!(record.id)),
            "legacyResourceId" => Some(json!(legacy_id)),
            "name" => Some(json!(record.name)),
            "query" => Some(json!(record.query)),
            "resourceType" => Some(json!(record.resource_type)),
            "searchTerms" => Some(json!("")),
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

fn saved_search_name_taken_user_error() -> Value {
    json!({
        "field": ["input", "name"],
        "message": "Name has already been taken"
    })
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

fn saved_search_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let mut arguments = root_field_arguments(query, variables)?;
    match arguments.remove("input") {
        Some(ResolvedValue::Object(input)) => Some(input),
        _ => None,
    }
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
                "status:open invoice_sent:true",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390630706",
                "Open",
                "status:open",
                "DRAFT_ORDER",
            ),
        ],
        _ => Vec::new(),
    }
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
    query
        .split_whitespace()
        .filter_map(|term| {
            let (key, value) = term.split_once(':')?;
            if key.is_empty() || value.is_empty() {
                None
            } else {
                Some((key.to_string(), value.to_string()))
            }
        })
        .collect()
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
    name.trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
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
    let mut values = match input.get(field) {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    values.sort();
    values
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

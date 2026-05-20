use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::graphql::{
    nested_root_field_path_selection, nested_root_field_selection, parse_operation,
    root_field_arguments, root_field_selection, root_fields, OperationType, ResolvedValue,
    RootFieldSelection, SelectedField,
};
use crate::operation_registry::{
    default_registry, operation_capability, CapabilityDomain, CapabilityExecution,
    OperationRegistryEntry,
};

pub const DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES: u64 = 104_857_600;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRecord {
    pub id: String,
    pub title: String,
    pub handle: String,
    pub status: String,
    pub description_html: String,
    pub vendor: String,
    pub product_type: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DraftProxy {
    config: Config,
    log_entries: Vec<Value>,
    registry: Vec<OperationRegistryEntry>,
    base_products: BTreeMap<String, ProductRecord>,
    staged_products: BTreeMap<String, ProductRecord>,
    staged_deleted_product_ids: BTreeSet<String>,
    next_synthetic_id: u64,
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
            next_synthetic_id: 1,
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
                ok_json(json!({ "ok": true, "message": "state reset" }))
            }
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
            "baseState": {},
            "stagedState": {}
        })
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

        let capability =
            operation_capability(&self.registry, operation.operation_type, Some(root_field));
        match (capability.domain, capability.execution) {
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if root_field == "product" && self.config.read_mode == ReadMode::Snapshot =>
            {
                ok_json(json!({ "data": { "product": self.product_by_id(&query, &variables) } }))
            }
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if root_field == "products" && self.config.read_mode == ReadMode::Snapshot =>
            {
                ok_json(
                    json!({ "data": { "products": self.products_connection(&query, &variables) } }),
                )
            }
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if root_field == "productsCount" && self.config.read_mode == ReadMode::Snapshot =>
            {
                ok_json(json!({ "data": { "productsCount": self.products_count(&query) } }))
            }
            (CapabilityDomain::Products, CapabilityExecution::OverlayRead)
                if root_field == "productByIdentifier"
                    && self.config.read_mode == ReadMode::Snapshot =>
            {
                ok_json(json!({
                    "data": self.product_by_identifier_fields(&query, &variables)
                }))
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productCreate" =>
            {
                self.product_create(&query, &variables)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productUpdate" =>
            {
                self.product_update(&query, &variables)
            }
            (CapabilityDomain::Products, CapabilityExecution::StageLocally)
                if root_field == "productDelete" =>
            {
                self.product_delete(&query, &variables)
            }
            (CapabilityDomain::Unknown, CapabilityExecution::Passthrough) => {
                match operation.operation_type {
                    OperationType::Query => json_error(
                        400,
                        &format!(
                            "No domain dispatcher implemented for root field: {}",
                            root_field
                        ),
                    ),
                    OperationType::Mutation => json_error(
                        400,
                        &format!(
                            "No mutation dispatcher implemented for root field: {}",
                            root_field
                        ),
                    ),
                    OperationType::Subscription => json_error(
                        400,
                        &format!(
                            "No domain dispatcher implemented for root field: {}",
                            root_field
                        ),
                    ),
                }
            }
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

    fn product_by_id(&self, query: &str, variables: &BTreeMap<String, ResolvedValue>) -> Value {
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let Some(ResolvedValue::String(id)) = arguments.get("id") else {
            return Value::Null;
        };
        if self.staged_deleted_product_ids.contains(id) {
            return Value::Null;
        }
        match self
            .staged_products
            .get(id)
            .or_else(|| self.base_products.get(id))
        {
            Some(product) => {
                product_json(product, &root_field_selection(query).unwrap_or_default())
            }
            None => Value::Null,
        }
    }

    fn product_by_identifier_fields(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut fields = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "productByIdentifier" {
                continue;
            }
            fields.insert(
                field.response_key.clone(),
                self.product_by_identifier_field(&field),
            );
        }
        Value::Object(fields)
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

    fn products_connection(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let root_selection = root_field_selection(query).unwrap_or_default();
        let node_selection = nested_root_field_selection(query, "nodes").unwrap_or_default();
        let edge_node_selection =
            nested_root_field_path_selection(query, &["edges", "node"]).unwrap_or_default();
        let page_info_selection =
            nested_root_field_selection(query, "pageInfo").unwrap_or_default();
        let limit = root_field_arguments(query, variables).and_then(|arguments| {
            match arguments.get("first") {
                Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as usize),
                _ => None,
            }
        });
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
                connection.insert(selection.response_key, value);
            }
        }

        Value::Object(connection)
    }

    fn products_count(&self, query: &str) -> Value {
        product_count_json(
            self.effective_product_count(),
            &root_field_selection(query).unwrap_or_default(),
        )
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
    ) -> Response {
        let Some(input) = product_create_input(query, variables) else {
            return ok_json(json!({
                "data": {
                    "productCreate": {
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
            return ok_json(json!({
                "data": {
                    "productCreate": {
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
        };
        self.staged_products.insert(id, product.clone());

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        ok_json(json!({
            "data": {
                "productCreate": product_mutation_payload_json(&product, &payload_selection, &product_selection)
            }
        }))
    }

    fn product_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
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
            return product_update_missing_product();
        };
        let Some(existing) = self
            .staged_products
            .get(&id)
            .or_else(|| self.base_products.get(&id))
            .cloned()
        else {
            return product_update_missing_product();
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
        };
        self.staged_products.insert(id, product.clone());

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        ok_json(json!({
            "data": {
                "productUpdate": product_mutation_payload_json(&product, &payload_selection, &product_selection)
            }
        }))
    }

    fn product_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(input) = product_input(query, variables) else {
            return product_delete_missing_product();
        };
        let Some(id) = resolved_string_field(&input, "id") else {
            return product_delete_missing_product();
        };
        if !self.staged_products.contains_key(&id) && !self.base_products.contains_key(&id) {
            return product_delete_missing_product();
        }

        self.staged_products.remove(&id);
        self.staged_deleted_product_ids.insert(id.clone());

        let payload_selection = root_field_selection(query).unwrap_or_default();
        ok_json(json!({
            "data": {
                "productDelete": product_delete_payload_json(&id, &payload_selection)
            }
        }))
    }

    fn next_proxy_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        format!("gid://shopify/{resource_type}/{id}?shopify-draft-proxy=synthetic")
    }
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
            _ => None,
        };
        if let Some(value) = value {
            fields.insert(selection.response_key.clone(), value);
        }
    }
    Value::Object(fields)
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

fn product_update_missing_product() -> Response {
    ok_json(json!({
        "data": {
            "productUpdate": {
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

fn product_delete_missing_product() -> Response {
    ok_json(json!({
        "data": {
            "productDelete": {
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

fn resolved_string_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Option<String> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_string_list_field(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Vec<String> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Health,
    MetaConfig,
    MetaLog,
    MetaState,
    MetaReset,
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

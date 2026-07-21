use super::owner_metafields::{
    metafield_cursor, owner_metafield_key_position, owner_metafield_with_connection_key,
    owner_metafields_connection_keys,
};
use super::*;
use base64::Engine as _;

const BULK_OPERATION_CURSORS_FIELD: &str = "__shopifyDraftProxyBulkOperationCursors";
const BULK_CATALOG_PAGE_SIZE: i64 = 250;
// Keep every polling request bounded while allowing ordinary captured Shopify
// jobs to move directly from CREATED to a terminal state on their first poll.
const BULK_CATALOG_PAGES_PER_READ: usize = 16;
const BULK_CATALOG_MAX_PAGES: usize = 10_000;

pub(in crate::proxy) fn bulk_operation_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy>
{
    vec![FieldResolverTypePolicy::property_backed_ordinary_fields(
        ApiSurface::Admin,
        "BulkOperation",
        "argument-bearing bulk-operation field has no explicit canonical resolver",
    )]
}

impl DraftProxy {
    pub(crate) fn bulk_operation_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        self.bulk_operation_read_outcome(&invocation, &arguments)
    }

    pub(crate) fn bulk_operation_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        match invocation.root_name {
            "bulkOperationRunQuery" => {
                self.bulk_operation_run_query_outcome(invocation.request, &arguments)
            }
            "bulkOperationRunMutation" => {
                self.bulk_operation_run_mutation_outcome(invocation.request, &arguments)
            }
            "bulkOperationCancel" => {
                self.bulk_operation_cancel_outcome(invocation.request, &arguments)
            }
            root => {
                ResolverOutcome::error(format!("Unknown bulk-operation mutation root `{root}`"))
            }
        }
    }
}

const BULK_OPERATION_HYDRATE_QUERY: &str = "query BulkOperationHydrate($id: ID!) { bulkOperation(id: $id) { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query } }";
const BULK_OPERATION_QUERY_STORAGE_BYTE_LIMIT: usize = 65_535;
const BULK_OPERATION_RUN_MUTATION_MAX_CONNECTIONS: usize = 1;
const BULK_OPERATION_RUN_MUTATION_MAX_CONNECTION_DEPTH: usize = 1;
const SUPPORTED_PRODUCT_BULK_CHILD_CONNECTIONS: &[&str] =
    &["collections", "images", "media", "metafields", "variants"];
const SUPPORTED_PRODUCT_VARIANT_BULK_CHILD_CONNECTIONS: &[&str] = &["media", "metafields"];

#[derive(Clone, Copy)]
struct BulkOperationRecordSpec<'a> {
    id: &'a str,
    status: &'a str,
    operation_type: &'a str,
    query: &'a str,
    count: &'a str,
    root_count: &'a str,
    created_at: &'a str,
    file_size: &'a str,
}

struct BulkOperationRunQueryResult {
    jsonl: String,
    root_object_count: usize,
}

struct BulkOperationRunMutationResult {
    jsonl: String,
    object_count: usize,
    status: &'static str,
}

enum BulkCatalogPageOutcome {
    More(String),
    Complete,
    Failed,
}

impl DraftProxy {
    pub(in crate::proxy) fn bulk_operation_result_jsonl(&self, artifact_id: &str) -> Response {
        let Some(result) = self.store.staged.bulk_operation_results.get(artifact_id) else {
            return json_error(404, "Not found");
        };
        Response {
            status: 200,
            headers: BTreeMap::from([(
                "content-type".to_string(),
                "application/jsonl; charset=utf-8".to_string(),
            )]),
            body: Value::String(result.clone()),
        }
    }

    fn bulk_operation_result_artifact_url(&self, id: &str) -> String {
        bulk_operation_result_artifact_url_for_port(self.config.port, id)
    }

    fn bulk_operation_record(&self, spec: BulkOperationRecordSpec<'_>) -> Value {
        bulk_operation_record_value(spec, self.bulk_operation_result_artifact_url(spec.id))
    }

    fn stage_bulk_operation_result(&mut self, id: &str, jsonl: String) {
        self.store
            .staged
            .bulk_operation_results
            .insert(resource_id_path_tail(id).to_string(), jsonl);
    }

    fn next_bulk_operation_gid(&mut self) -> String {
        let id = shopify_gid(
            "BulkOperation",
            7_000_000_000_000_u64 + self.next_synthetic_id,
        );
        self.next_synthetic_id += 1;
        id
    }

    fn bulk_operation_run_query_result(
        &self,
        query_text: &str,
        api_version: crate::admin_graphql::AdminApiVersion,
    ) -> BulkOperationRunQueryResult {
        let Some(document) = parsed_document(query_text, &BTreeMap::new()) else {
            return BulkOperationRunQueryResult {
                jsonl: String::new(),
                root_object_count: 0,
            };
        };
        let Some(field) = document.root_fields.first() else {
            return BulkOperationRunQueryResult {
                jsonl: String::new(),
                root_object_count: 0,
            };
        };

        match field.name.as_str() {
            "products" => self.bulk_operation_products_result(field, api_version),
            "productVariants" => self.bulk_operation_product_variants_result(field, api_version),
            _ => BulkOperationRunQueryResult {
                jsonl: String::new(),
                root_object_count: 0,
            },
        }
    }

    fn hydrate_bulk_query_catalog_page(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
        after: Option<&str>,
        seen_cursors: &BTreeSet<String>,
    ) -> BulkCatalogPageOutcome {
        let (operation_name, root_name) = match field.name.as_str() {
            "products" => ("BulkProductsCatalogHydrate", "products"),
            "productVariants" => ("BulkProductVariantsCatalogHydrate", "productVariants"),
            _ => return BulkCatalogPageOutcome::Failed,
        };
        let plan = bulk_catalog_hydration_plan(operation_name, root_name, field);
        let response = self.upstream_post(
            request,
            json!({
                "query": plan.query,
                "operationName": operation_name,
                "variables": {
                    "first": BULK_CATALOG_PAGE_SIZE,
                    "after": after,
                    "nestedFirst": BULK_CATALOG_PAGE_SIZE,
                    "nestedAfter": null,
                }
            }),
        );
        if !(200..300).contains(&response.status)
            || response
                .body
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
        {
            return BulkCatalogPageOutcome::Failed;
        }
        let Some(connection) = response.body.pointer(&format!("/data/{root_name}")) else {
            return BulkCatalogPageOutcome::Failed;
        };
        let Some(nodes) = connection.get("nodes").and_then(Value::as_array) else {
            return BulkCatalogPageOutcome::Failed;
        };
        let mut normalized_nodes = Vec::with_capacity(nodes.len());
        for node in nodes {
            let mut node = node.clone();
            if !normalize_complete_bulk_catalog_nested_connections(
                &mut node,
                &plan.nested_connections,
            ) {
                return BulkCatalogPageOutcome::Failed;
            }
            normalized_nodes.push(node);
        }
        for node in &normalized_nodes {
            if !self.observe_bulk_catalog_node(root_name, node) {
                return BulkCatalogPageOutcome::Failed;
            }
        }
        match connection["pageInfo"]["hasNextPage"].as_bool() {
            Some(false) => BulkCatalogPageOutcome::Complete,
            Some(true) => {
                let Some(end_cursor) = connection["pageInfo"]["endCursor"].as_str() else {
                    return BulkCatalogPageOutcome::Failed;
                };
                if seen_cursors.contains(end_cursor) {
                    return BulkCatalogPageOutcome::Failed;
                }
                BulkCatalogPageOutcome::More(end_cursor.to_string())
            }
            None => BulkCatalogPageOutcome::Failed,
        }
    }

    fn observe_bulk_catalog_node(&mut self, root_name: &str, node: &Value) -> bool {
        match root_name {
            "products" => {
                let Some(product_id) = node.get("id").and_then(Value::as_str) else {
                    return false;
                };
                self.store.observe_base_product_json(node);
                for variant in node
                    .pointer("/variants/nodes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    self.store
                        .observe_base_product_variant_json(variant, product_id);
                }
                true
            }
            "productVariants" => {
                let Some(product) = node.get("product") else {
                    return false;
                };
                let Some(product_id) = product.get("id").and_then(Value::as_str) else {
                    return false;
                };
                self.store.observe_base_product_json(product);
                self.store
                    .observe_base_product_variant_json(node, product_id);
                true
            }
            _ => false,
        }
    }

    fn bulk_operation_products_result(
        &self,
        field: &RootFieldSelection,
        api_version: crate::admin_graphql::AdminApiVersion,
    ) -> BulkOperationRunQueryResult {
        let products = self.products_filtered_by_search_query(field.arguments.get("query"));
        let root_object_count = products.len();

        let node_selection = edge_node_selection(&field.selection);
        let product_selection = bulk_jsonl_node_selection(&node_selection);
        let nested_connections = node_selection
            .iter()
            .filter(|selection| {
                product_bulk_child_connection_supported(&selection.name)
                    && field_is_selected(&selection.selection, "edges")
            })
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        for product in products {
            let variants = self.store.product_variants_for_product(&product.id);
            let product_json = bulk_project_value(
                &self.product_canonical_value(&product),
                &product_selection,
                api_version,
            );
            rows.push(product_json);

            for selection in &nested_connections {
                for child in
                    self.bulk_jsonl_product_child_rows(&product, &variants, selection, api_version)
                {
                    rows.push(bulk_jsonl_child_node(child, &product.id));
                }
            }
        }

        BulkOperationRunQueryResult {
            jsonl: values_to_jsonl(rows),
            root_object_count,
        }
    }

    fn bulk_jsonl_product_child_rows(
        &self,
        product: &ProductRecord,
        variants: &[ProductVariantRecord],
        selection: &SelectedField,
        api_version: crate::admin_graphql::AdminApiVersion,
    ) -> Vec<Value> {
        let child_node_selection = edge_node_selection(&selection.selection);
        let child_node_selection = bulk_jsonl_node_selection(&child_node_selection);
        if child_node_selection.is_empty() {
            return Vec::new();
        }

        let rows = match selection.name.as_str() {
            "collections" => product
                .collections
                .iter()
                .map(|collection| {
                    bulk_project_value(collection, &child_node_selection, api_version)
                })
                .collect(),
            "images" => product
                .extra_fields
                .get("images")
                .map(connection_nodes)
                .unwrap_or_else(|| {
                    product
                        .media
                        .iter()
                        .filter_map(product_image_json_from_media)
                        .collect()
                })
                .iter()
                .map(|image| bulk_project_value(image, &child_node_selection, api_version))
                .collect(),
            "media" => product
                .media
                .iter()
                .map(|media| bulk_project_value(media, &child_node_selection, api_version))
                .collect(),
            "metafields" => self
                .bulk_owner_metafield_nodes(
                    &product.id,
                    product.extra_fields.get("metafields"),
                    selection,
                )
                .into_iter()
                .map(|metafield| {
                    self.bulk_metafield_value(metafield, &child_node_selection, api_version)
                })
                .collect(),
            "variants" => variants
                .iter()
                .map(|variant| self.product_variant_canonical_value(variant))
                .collect(),
            _ => Vec::new(),
        };
        rows.into_iter()
            .map(|row| bulk_project_value(&row, &child_node_selection, api_version))
            .collect()
    }

    fn bulk_metafield_value(
        &self,
        mut metafield: Value,
        selection: &[SelectedField],
        api_version: crate::admin_graphql::AdminApiVersion,
    ) -> Value {
        for field in selection {
            match field.name.as_str() {
                "reference" => {
                    metafield["reference"] =
                        self.canonical_metafield_reference_value(&metafield, None);
                }
                "references" => {
                    metafield["references"] = self.canonical_metafield_references_connection_value(
                        &metafield,
                        &field.arguments,
                        None,
                    );
                }
                _ => {}
            }
        }
        bulk_project_value(&metafield, selection, api_version)
    }

    fn bulk_owner_metafield_nodes(
        &self,
        owner_id: &str,
        base_metafields: Option<&Value>,
        selection: &SelectedField,
    ) -> Vec<Value> {
        let namespace = resolved_string_field(&selection.arguments, "namespace");
        let keys = owner_metafields_connection_keys(&selection.arguments);
        let has_local_effects = self
            .store
            .staged
            .owner_metafields
            .get(owner_id)
            .is_some_and(|metafields| !metafields.is_empty())
            || self
                .store
                .staged
                .deleted_owner_metafields
                .iter()
                .any(|(deleted_owner_id, _, _)| deleted_owner_id == owner_id);

        let mut records = if has_local_effects {
            self.owner_metafields(owner_id, namespace.as_deref(), keys.as_deref())
        } else {
            let mut records = base_metafields
                .map(connection_nodes)
                .unwrap_or_default()
                .into_iter()
                .filter(|metafield| {
                    let metafield_namespace = metafield.get("namespace").and_then(Value::as_str);
                    let metafield_key = metafield.get("key").and_then(Value::as_str);
                    namespace
                        .as_deref()
                        .is_none_or(|namespace| metafield_namespace == Some(namespace))
                        && keys.as_deref().is_none_or(|keys: &[(String, String)]| {
                            matches!(
                                (metafield_namespace, metafield_key),
                                (Some(namespace), Some(key))
                                    if keys.iter().any(|(filter_namespace, filter_key)| {
                                        filter_namespace == namespace && filter_key == key
                                    })
                            )
                        })
                })
                .collect::<Vec<_>>();
            if let Some(keys) = keys.as_deref() {
                records.sort_by_key(|metafield| owner_metafield_key_position(metafield, keys));
            }
            records
        };

        if resolved_bool_field(&selection.arguments, "reverse").unwrap_or(false) {
            records.reverse();
        }
        let (records, _) = connection_window(&records, &selection.arguments, |metafield| {
            metafield_cursor(metafield).unwrap_or_default()
        });
        if keys.is_some() {
            records
                .into_iter()
                .map(owner_metafield_with_connection_key)
                .collect()
        } else {
            records
        }
    }

    fn bulk_operation_product_variants_result(
        &self,
        field: &RootFieldSelection,
        api_version: crate::admin_graphql::AdminApiVersion,
    ) -> BulkOperationRunQueryResult {
        let products = self.products_filtered_by_search_query(field.arguments.get("query"));
        let node_selection = edge_node_selection(&field.selection);
        let variant_selection = bulk_jsonl_node_selection(&node_selection);
        let nested_connections = node_selection
            .iter()
            .filter(|selection| {
                product_variant_bulk_child_connection_supported(&selection.name)
                    && field_is_selected(&selection.selection, "edges")
            })
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        let mut root_object_count = 0;
        for product in products {
            for variant in self.store.product_variants_for_product(&product.id) {
                root_object_count += 1;
                let mut value = self.product_variant_canonical_value(&variant);
                value["product"] = self.product_canonical_value(&product);
                rows.push(bulk_project_value(&value, &variant_selection, api_version));

                for selection in &nested_connections {
                    for child in self.bulk_jsonl_product_variant_child_rows(
                        &product,
                        &variant,
                        selection,
                        api_version,
                    ) {
                        rows.push(bulk_jsonl_child_node(child, &variant.id));
                    }
                }
            }
        }

        BulkOperationRunQueryResult {
            jsonl: values_to_jsonl(rows),
            root_object_count,
        }
    }

    fn bulk_jsonl_product_variant_child_rows(
        &self,
        product: &ProductRecord,
        variant: &ProductVariantRecord,
        selection: &SelectedField,
        api_version: crate::admin_graphql::AdminApiVersion,
    ) -> Vec<Value> {
        let child_node_selection = edge_node_selection(&selection.selection);
        let child_node_selection = bulk_jsonl_node_selection(&child_node_selection);
        if child_node_selection.is_empty() {
            return Vec::new();
        }

        let rows = match selection.name.as_str() {
            "media" => variant
                .extra_fields
                .get("media")
                .map(connection_nodes)
                .filter(|media| !media.is_empty())
                .unwrap_or_else(|| variant_attached_media_nodes(variant, Some(product)))
                .iter()
                .map(|media| bulk_project_value(media, &child_node_selection, api_version))
                .collect(),
            "metafields" => self
                .bulk_owner_metafield_nodes(
                    &variant.id,
                    variant.extra_fields.get("metafields"),
                    selection,
                )
                .into_iter()
                .map(|metafield| {
                    self.bulk_metafield_value(metafield, &child_node_selection, api_version)
                })
                .collect(),
            _ => Vec::new(),
        };
        rows.into_iter()
            .map(|row| bulk_project_value(&row, &child_node_selection, api_version))
            .collect()
    }

    pub(in crate::proxy) fn bulk_operation_read_outcome(
        &mut self,
        invocation: &RootInvocation<'_>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        if let Some(errors) = bulk_operation_read_validation_errors(
            invocation.root_name,
            invocation.response_key,
            arguments,
            invocation.root_location,
            invocation.operation_path,
        ) {
            return graphql_error_outcome(errors, invocation.response_key);
        }

        self.advance_bulk_query_execution_for_read(
            invocation.request,
            invocation.root_name,
            arguments,
        );

        let upstream_outcome =
            if self.bulk_operation_read_needs_upstream(invocation.root_name, arguments) {
                let result = self.cached_or_forward_upstream_graphql_result(
                    invocation.request,
                    invocation.response_key,
                );
                if !result.transport_succeeded {
                    return result.outcome;
                }
                self.observe_bulk_operation_read_result(
                    invocation.response_key,
                    invocation.root_name,
                    arguments,
                    &result.data,
                );
                Some(result.outcome)
            } else {
                None
            };

        let value = match invocation.root_name {
            "bulkOperation" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                self.bulk_operation_by_id(&id)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "bulkOperations" => self.bulk_operations_connection_value(arguments),
            "currentBulkOperation" => {
                let operation_type =
                    resolved_string_field(arguments, "type").unwrap_or_else(|| "QUERY".to_string());
                self.current_bulk_operation(&operation_type)
                    .unwrap_or(Value::Null)
            }
            _ => {
                return resolver_http_error_outcome(
                    501,
                    format!(
                        "Unsupported bulk operation read root: {}",
                        invocation.root_name
                    ),
                );
            }
        };
        let mut outcome = ResolverOutcome::value(value);
        if let Some(search) = bulk_operation_search_extension(
            invocation.response_key,
            invocation.root_name,
            arguments,
        ) {
            outcome.extensions.insert("search".to_string(), search);
        }
        if let Some(upstream) = upstream_outcome {
            outcome.errors = upstream.errors;
            outcome.extensions.extend(upstream.extensions);
        }
        outcome
    }

    fn bulk_operation_read_needs_upstream(
        &self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return false;
        }
        match root_field {
            "bulkOperation" => {
                let id = resolved_string_field(arguments, "id").unwrap_or_default();
                self.bulk_operation_by_id(&id).is_none()
            }
            "bulkOperations" => self.bulk_operations_connection_needs_upstream(arguments),
            "currentBulkOperation" => {
                let operation_type =
                    resolved_string_field(arguments, "type").unwrap_or_else(|| "QUERY".to_string());
                self.current_bulk_operation(&operation_type).is_none()
            }
            _ => false,
        }
    }

    fn bulk_operations_connection_needs_upstream(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        if !self.store.base.bulk_operations_observed {
            let requested = arguments
                .get("first")
                .and_then(|value| match value {
                    ResolvedValue::Int(first) => Some(*first),
                    _ => None,
                })
                .filter(|first| *first >= 0)
                .map(|first| first as usize);
            let matching_staged = self
                .store
                .staged
                .bulk_operations
                .values()
                .filter(|operation| bulk_operation_matches_query(operation, arguments))
                .count();
            // A bounded window that can be filled entirely from locally staged
            // operations is authoritative. Larger or unbounded windows still
            // need the upstream catalog so staging one operation does not hide
            // all pre-existing operations.
            return requested.is_none_or(|requested| matching_staged < requested);
        }
        let sort_key = bulk_operation_connection_sort_key(arguments);
        self.store
            .base
            .bulk_operations
            .ordered_values()
            .into_iter()
            .any(|operation| bulk_operation_observed_cursor(operation, &sort_key).is_none())
    }

    fn observe_bulk_operation_read_result(
        &mut self,
        response_key: &str,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        data: &Value,
    ) {
        let Some(value) = data.get(response_key) else {
            return;
        };
        match root_field {
            "bulkOperation" | "currentBulkOperation" => {
                self.observe_bulk_operation_value(value);
            }
            "bulkOperations" => {
                self.store.base.bulk_operations_observed = true;
                self.observe_bulk_operations_connection(arguments, value);
            }
            _ => {}
        }
    }

    fn observe_bulk_operations_connection(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        connection: &Value,
    ) {
        let sort_key = bulk_operation_connection_sort_key(arguments);
        if let Some(edges) = connection.get("edges").and_then(Value::as_array) {
            for edge in edges {
                if let Some(node) = edge.get("node") {
                    self.observe_bulk_operation_value_with_cursor(
                        node,
                        edge.get("cursor")
                            .and_then(Value::as_str)
                            .map(|cursor| (sort_key.as_str(), cursor)),
                    );
                }
            }
        }
        if let Some(nodes) = connection.get("nodes").and_then(Value::as_array) {
            let start_cursor = connection
                .get("pageInfo")
                .and_then(|page_info| page_info.get("startCursor"))
                .and_then(Value::as_str);
            let end_cursor = connection
                .get("pageInfo")
                .and_then(|page_info| page_info.get("endCursor"))
                .and_then(Value::as_str);
            let last_index = nodes.len().saturating_sub(1);
            for (index, node) in nodes.iter().enumerate() {
                let cursor = if index == 0 {
                    start_cursor
                } else if index == last_index {
                    end_cursor
                } else {
                    None
                };
                self.observe_bulk_operation_value_with_cursor(
                    node,
                    cursor.map(|cursor| (sort_key.as_str(), cursor)),
                );
            }
        }
    }

    fn observe_bulk_operation_value(&mut self, operation: &Value) {
        self.observe_bulk_operation_value_with_cursor(operation, None);
    }

    fn observe_bulk_operation_value_with_cursor(
        &mut self,
        operation: &Value,
        cursor: Option<(&str, &str)>,
    ) {
        if !operation.is_object() {
            return;
        }
        let Some(id) = operation.get("id").and_then(Value::as_str) else {
            return;
        };
        if shopify_gid_resource_type(id) != Some("BulkOperation") {
            return;
        }
        let mut operation = operation.clone();
        if let Some(object) = operation.as_object_mut() {
            if let Some(existing_cursors) = self
                .store
                .base
                .bulk_operations
                .get(id)
                .and_then(|operation| operation.get(BULK_OPERATION_CURSORS_FIELD))
            {
                object.insert(
                    BULK_OPERATION_CURSORS_FIELD.to_string(),
                    existing_cursors.clone(),
                );
            }
            if let Some((sort_key, cursor)) = cursor {
                let cursors = object
                    .entry(BULK_OPERATION_CURSORS_FIELD.to_string())
                    .or_insert_with(|| json!({}));
                if let Some(cursors) = cursors.as_object_mut() {
                    cursors.insert(sort_key.to_string(), json!(cursor));
                }
            }
        }
        self.store
            .base
            .bulk_operations
            .insert(id.to_string(), operation);
    }

    fn bulk_operation_by_id(&self, id: &str) -> Option<&Value> {
        effective_get(
            &self.store.base.bulk_operations,
            &self.store.staged.bulk_operations,
            id,
        )
    }

    fn effective_bulk_operations(&self) -> Vec<Value> {
        let mut operations = effective_records(
            &self.store.base.bulk_operations,
            &self.store.staged.bulk_operations,
        );
        operations.sort_by(|left, right| {
            bulk_operation_sort_value(right, "CREATED_AT")
                .cmp(&bulk_operation_sort_value(left, "CREATED_AT"))
                .then_with(|| {
                    right
                        .get("id")
                        .and_then(Value::as_str)
                        .cmp(&left.get("id").and_then(Value::as_str))
                })
        });
        operations
    }

    fn current_bulk_operation(&self, operation_type: &str) -> Option<Value> {
        self.effective_bulk_operations()
            .into_iter()
            .find(|operation| operation.get("type").and_then(Value::as_str) == Some(operation_type))
    }

    fn bulk_operations_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut operations = self.effective_bulk_operations();
        operations.retain(|operation| bulk_operation_matches_query(operation, arguments));

        let sort_key = bulk_operation_connection_sort_key(arguments);
        operations.sort_by(|left, right| {
            bulk_operation_sort_value(right, &sort_key)
                .cmp(&bulk_operation_sort_value(left, &sort_key))
                .then_with(|| {
                    right
                        .get("id")
                        .and_then(Value::as_str)
                        .cmp(&left.get("id").and_then(Value::as_str))
                })
        });
        connection_value_with_args(operations, arguments, |operation| {
            bulk_operation_connection_cursor(operation, &sort_key)
        })
    }

    fn advance_bulk_query_execution_for_read(
        &mut self,
        request: &Request,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) {
        if self.execution_session.bulk_query_execution_advanced {
            return;
        }
        self.execution_session.bulk_query_execution_advanced = true;

        let requested_id = (root_name == "bulkOperation")
            .then(|| resolved_string_field(arguments, "id"))
            .flatten();
        let execution_id = if let Some(requested_id) = requested_id {
            self.store
                .staged
                .bulk_query_executions
                .contains_key(&requested_id)
                .then_some(requested_id)
        } else {
            self.store
                .staged
                .bulk_query_executions
                .keys()
                .next()
                .cloned()
        };
        let Some(execution_id) = execution_id else {
            return;
        };
        self.advance_bulk_query_execution(request, &execution_id);
    }

    fn advance_bulk_query_execution(&mut self, request: &Request, id: &str) {
        let Some(mut execution) = self.store.staged.bulk_query_executions.get(id).cloned() else {
            return;
        };
        self.state_revision = self.state_revision.saturating_add(1);
        if execution.cancel_requested {
            self.cancel_pending_bulk_query_execution(id);
            return;
        }

        if self.config.read_mode == ReadMode::Snapshot {
            self.complete_bulk_query_execution(id, &execution);
            return;
        }
        let Some(document) = parsed_document(&execution.query, &BTreeMap::new()) else {
            self.fail_bulk_query_execution(id);
            return;
        };
        let Some(field) = document.root_fields.first() else {
            self.fail_bulk_query_execution(id);
            return;
        };
        self.mark_bulk_query_running(id);
        let execution_request = Request {
            method: "POST".to_string(),
            path: execution.request_path.clone(),
            headers: request.headers.clone(),
            body: String::new(),
        };
        for _ in 0..BULK_CATALOG_PAGES_PER_READ {
            if execution.page_count >= BULK_CATALOG_MAX_PAGES {
                self.fail_bulk_query_execution(id);
                return;
            }
            match self.hydrate_bulk_query_catalog_page(
                &execution_request,
                field,
                execution.after.as_deref(),
                &execution.seen_cursors,
            ) {
                BulkCatalogPageOutcome::More(cursor) => {
                    execution.after = Some(cursor.clone());
                    execution.seen_cursors.insert(cursor);
                    execution.page_count += 1;
                    self.store
                        .staged
                        .bulk_query_executions
                        .insert(id.to_string(), execution.clone());
                }
                BulkCatalogPageOutcome::Complete => {
                    self.complete_bulk_query_execution(id, &execution);
                    return;
                }
                BulkCatalogPageOutcome::Failed => {
                    self.fail_bulk_query_execution(id);
                    return;
                }
            }
        }
    }

    fn mark_bulk_query_running(&mut self, id: &str) {
        let Some(mut operation) = self.bulk_operation_by_id(id).cloned() else {
            return;
        };
        operation["status"] = json!("RUNNING");
        self.store
            .staged
            .bulk_operations
            .insert(id.to_string(), operation);
    }

    fn complete_bulk_query_execution(&mut self, id: &str, execution: &BulkQueryExecution) {
        let api_version =
            crate::admin_graphql::AdminApiVersion::from_route(&execution.request_path)
                .unwrap_or(crate::admin_graphql::AdminApiVersion::DEFAULT);
        let result = self.bulk_operation_run_query_result(&execution.query, api_version);
        let (object_count, file_size) = bulk_operation_result_metadata(&result.jsonl);
        let root_object_count = result.root_object_count.to_string();
        let created_at = self
            .bulk_operation_by_id(id)
            .and_then(|operation| operation.get("createdAt"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let operation = self.bulk_operation_record(BulkOperationRecordSpec {
            id,
            status: "COMPLETED",
            operation_type: "QUERY",
            query: &execution.query,
            count: &object_count,
            root_count: &root_object_count,
            created_at: &created_at,
            file_size: &file_size,
        });
        self.stage_bulk_operation_result(id, result.jsonl);
        self.store
            .staged
            .bulk_operations
            .insert(id.to_string(), operation);
        self.store.staged.bulk_query_executions.remove(id);
    }

    fn fail_bulk_query_execution(&mut self, id: &str) {
        let Some(mut operation) = self.bulk_operation_by_id(id).cloned() else {
            self.store.staged.bulk_query_executions.remove(id);
            return;
        };
        operation["status"] = json!("FAILED");
        operation["errorCode"] = json!("INTERNAL_SERVER_ERROR");
        operation["completedAt"] = operation["createdAt"].clone();
        operation["objectCount"] = json!("0");
        operation["rootObjectCount"] = json!("0");
        operation["fileSize"] = Value::Null;
        operation["url"] = Value::Null;
        operation["partialDataUrl"] = Value::Null;
        self.store
            .staged
            .bulk_operations
            .insert(id.to_string(), operation);
        self.store.staged.bulk_query_executions.remove(id);
        self.store
            .staged
            .bulk_operation_results
            .remove(resource_id_path_tail(id));
    }

    fn cancel_pending_bulk_query_execution(&mut self, id: &str) {
        let Some(mut operation) = self.bulk_operation_by_id(id).cloned() else {
            self.store.staged.bulk_query_executions.remove(id);
            return;
        };
        operation["status"] = json!("CANCELED");
        operation["errorCode"] = Value::Null;
        operation["completedAt"] = Value::Null;
        operation["objectCount"] = json!("0");
        operation["rootObjectCount"] = json!("0");
        operation["fileSize"] = Value::Null;
        operation["url"] = Value::Null;
        operation["partialDataUrl"] = Value::Null;
        self.store
            .staged
            .bulk_operations
            .insert(id.to_string(), operation);
        self.store.staged.bulk_query_executions.remove(id);
        self.store
            .staged
            .bulk_operation_results
            .remove(resource_id_path_tail(id));
    }

    pub(in crate::proxy) fn bulk_operation_run_query_outcome(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let query_text = resolved_string_field(arguments, "query").unwrap_or_else(|| {
            "#graphql\n{ products { edges { node { id title } } } }".to_string()
        });
        if let Some(user_errors) = bulk_operation_run_query_user_errors(&query_text) {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": user_errors
            });
            return ResolverOutcome::value(payload);
        }
        if let Some(operation_id) = self.throttled_bulk_operation_id("QUERY", request) {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [user_error(
                    Value::Null,
                    &format!("A bulk query operation for this app and shop is already in progress: {operation_id}."),
                    Some("OPERATION_IN_PROGRESS"),
                )]
            });
            return ResolverOutcome::value(payload);
        }

        // Shopify validates bulk queries against the Admin GraphQL schema, so the proxy
        // accepts schema-valid roots beyond the ones it can synthesize JSONL for locally.
        // Local synthesis is scoped to `products`/`productVariants`; every other accepted
        // root must fail explicitly because starting a real upstream bulk operation before
        // commit would violate the stage-locally mutation contract.
        let root_name = bulk_query_root_field_name(&query_text);
        let locally_synthesized = matches!(
            root_name.as_deref(),
            Some("products") | Some("productVariants")
        );
        if !locally_synthesized {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [unsupported_bulk_query_root_error(
                    root_name.as_deref().unwrap_or_default()
                )]
            });
            return ResolverOutcome::value(payload);
        }
        if let Some(user_errors) = bulk_operation_run_query_local_support_user_errors(&query_text) {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": user_errors
            });
            return ResolverOutcome::value(payload);
        }

        let id = self.next_bulk_operation_gid();
        let created_at = self.next_product_timestamp();
        let created_operation = self.bulk_operation_record(BulkOperationRecordSpec {
            id: &id,
            status: "CREATED",
            operation_type: "QUERY",
            query: &query_text,
            count: "0",
            root_count: "0",
            created_at: &created_at,
            file_size: "0",
        });
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), created_operation.clone());
        self.store.staged.bulk_query_executions.insert(
            id.clone(),
            BulkQueryExecution {
                query: query_text,
                request_path: request.path.clone(),
                after: None,
                seen_cursors: BTreeSet::new(),
                page_count: 0,
                cancel_requested: false,
            },
        );
        let payload = json!({
            "bulkOperation": created_operation,
            "userErrors": []
        });
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "bulkOperationRunQuery",
            "bulk-operations",
            vec![id],
        ))
    }

    pub(in crate::proxy) fn bulk_operation_run_mutation_outcome(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let mutation_text = resolved_string_field(arguments, "mutation").unwrap_or_default();
        let staged_upload_path =
            resolved_string_field(arguments, "stagedUploadPath").unwrap_or_default();
        let client_identifier = resolved_string_field(arguments, "clientIdentifier");

        let api_version = admin_graphql_version(&request.path)
            .unwrap_or_else(|| latest_supported_admin_graphql_version().unwrap_or("2026-04"));
        if let Some(user_errors) =
            bulk_operation_run_mutation_document_user_errors(&mutation_text, api_version)
        {
            return bulk_operation_run_mutation_error_outcome(user_errors);
        }
        if let Some(user_errors) =
            bulk_operation_run_mutation_client_identifier_user_errors(client_identifier.as_deref())
        {
            return bulk_operation_run_mutation_error_outcome(user_errors);
        }
        let staged_upload_file_size = self.bulk_operation_staged_upload_size(&staged_upload_path);
        let max_file_size = self
            .config
            .bulk_operation_run_mutation_max_input_file_size_bytes
            .unwrap_or(DEFAULT_BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES);
        if staged_upload_file_size
            .flatten()
            .is_some_and(|file_size| file_size > max_file_size)
        {
            return bulk_operation_run_mutation_error_outcome(vec![
                bulk_operation_run_mutation_file_size_too_large_user_error(max_file_size),
            ]);
        }
        if staged_upload_file_size.flatten() == Some(0) {
            return bulk_operation_run_mutation_error_outcome(vec![
                bulk_operation_run_mutation_empty_file_user_error(),
            ]);
        }
        if staged_upload_file_size.is_none() {
            return bulk_operation_run_mutation_error_outcome(vec![
                bulk_operation_run_mutation_no_such_file_user_error(),
            ]);
        }
        let Some(staged_upload_body) = self
            .bulk_operation_staged_upload_body(&staged_upload_path)
            .map(str::to_string)
        else {
            return bulk_operation_run_mutation_error_outcome(vec![
                bulk_operation_run_mutation_no_such_file_user_error(),
            ]);
        };
        if let Some(operation_id) = self.throttled_bulk_operation_id("MUTATION", request) {
            return bulk_operation_run_mutation_error_outcome(vec![user_error(
                    Value::Null,
                    &format!("A bulk mutation operation for this app and shop is already in progress: {operation_id}."),
                    Some("OPERATION_IN_PROGRESS"),
                )]);
        }

        let id = self.next_bulk_operation_gid();
        let created_at = self.next_product_timestamp();
        let result =
            self.bulk_operation_run_mutation_result(request, &mutation_text, &staged_upload_body);
        let object_count = result.object_count.to_string();
        let file_size = result.jsonl.len().to_string();
        let terminal_operation = self.bulk_operation_record(BulkOperationRecordSpec {
            id: &id,
            status: result.status,
            operation_type: "MUTATION",
            query: &mutation_text,
            count: &object_count,
            root_count: &object_count,
            created_at: &created_at,
            file_size: &file_size,
        });
        self.stage_bulk_operation_result(&id, result.jsonl);
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), terminal_operation);

        let payload = json!({
            "bulkOperation": self.bulk_operation_record(BulkOperationRecordSpec {
                id: &id,
                status: "CREATED",
                operation_type: "MUTATION",
                query: &mutation_text,
                count: "0",
                root_count: "0",
                created_at: &created_at,
                file_size: "0",
            }),
            "userErrors": []
        });
        ResolverOutcome::value(payload)
    }

    fn throttled_bulk_operation_id(
        &self,
        operation_type: &str,
        request: &Request,
    ) -> Option<String> {
        let mut operation_ids = self
            .store
            .staged
            .bulk_operations
            .iter()
            .filter(|(_, operation)| {
                operation.get("type").and_then(Value::as_str) == Some(operation_type)
                    && bulk_operation_is_non_terminal(operation)
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();

        if operation_ids.len() < bulk_operation_concurrent_limit(request) {
            return None;
        }

        operation_ids.sort();
        Some(operation_ids.join(", "))
    }

    fn bulk_operation_staged_upload_size(&self, staged_upload_path: &str) -> Option<Option<u64>> {
        let declared = self
            .store
            .staged
            .bulk_operation_staged_uploads
            .get(staged_upload_path)
            .cloned()?;
        if let Some(body) = self.bulk_operation_staged_upload_body(staged_upload_path) {
            return Some(Some(body.len() as u64));
        }
        Some(declared)
    }

    fn bulk_operation_staged_upload_body(&self, staged_upload_path: &str) -> Option<&str> {
        self.store
            .staged
            .bulk_operation_staged_upload_bodies
            .get(staged_upload_path)
            .map(String::as_str)
    }

    fn bulk_operation_run_mutation_result(
        &mut self,
        request: &Request,
        mutation_text: &str,
        jsonl: &str,
    ) -> BulkOperationRunMutationResult {
        let api_version = crate::admin_graphql::AdminApiVersion::from_route(&request.path)
            .unwrap_or(crate::admin_graphql::AdminApiVersion::DEFAULT);
        let inner_root = parsed_document(mutation_text, &BTreeMap::new())
            .and_then(|document| document.root_fields.into_iter().next())
            .map(|root| root.name)
            .expect("bulk mutation document validation guarantees one mutation root");
        let locally_implemented = self
            .registry
            .registration(OperationType::Mutation, &inner_root)
            .is_some_and(|registration| {
                registration.execution == CapabilityExecution::StageLocally
            });
        let unsupported_message = (!locally_implemented).then(|| {
            format!(
                "Bulk mutation root `{inner_root}` is accepted by Shopify but is not implemented locally. The proxy did not send this mutation upstream during draft staging."
            )
        });
        let mut rows = Vec::new();
        let mut object_count = 0usize;
        let mut failed = unsupported_message.is_some();
        for (line_number, line) in jsonl.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let variables = match serde_json::from_str::<Value>(line) {
                Ok(Value::Object(variables)) => Value::Object(variables),
                Ok(other) => {
                    failed = true;
                    rows.push(bulk_operation_run_mutation_line_error(
                        line_number,
                        &format!("Expected JSON object variables, got {other}"),
                    ));
                    continue;
                }
                Err(error) => {
                    failed = true;
                    rows.push(bulk_operation_run_mutation_line_error(
                        line_number,
                        &format!("Failed to parse JSONL variables: {error}"),
                    ));
                    continue;
                }
            };
            if let Some(message) = unsupported_message.as_deref() {
                rows.push(bulk_operation_run_mutation_line_error(line_number, message));
                continue;
            }
            let row_request = Request {
                method: "POST".to_string(),
                path: request.path.clone(),
                headers: request.headers.clone(),
                body: json!({
                    "query": mutation_text,
                    "variables": variables
                })
                .to_string(),
            };
            let mut row = bulk_project_mutation_response(
                self.resolve_nested_graphql_request(&row_request).body,
                mutation_text,
                &variables,
                api_version,
            );
            if let Some(object) = row.as_object_mut() {
                object.insert("__lineNumber".to_string(), json!(line_number));
            } else {
                row = json!({
                    "data": row,
                    "__lineNumber": line_number
                });
            }
            rows.push(row);
            object_count += 1;
        }
        BulkOperationRunMutationResult {
            jsonl: values_to_jsonl(rows),
            object_count,
            status: if failed { "FAILED" } else { "COMPLETED" },
        }
    }

    pub(in crate::proxy) fn bulk_operation_cancel_outcome(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();

        if self.bulk_operation_by_id(&id).is_none() {
            self.hydrate_bulk_operation_for_cancel(request, &id);
        }

        let Some(existing_operation) = self.bulk_operation_by_id(&id).cloned() else {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [user_error_omit_code(
                    ["id"],
                    "Bulk operation does not exist",
                    None,
                )]
            });
            return ResolverOutcome::value(payload);
        };

        let status = existing_operation
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if bulk_operation_status_is_terminal(Some(status)) {
            let payload = json!({
                "bulkOperation": existing_operation,
                "userErrors": [user_error_omit_code(
                    Value::Null,
                    &format!(
                        "A bulk operation cannot be canceled when it is {}",
                        status.to_ascii_lowercase()
                    ),
                    None,
                )]
            });
            return ResolverOutcome::value(payload);
        }

        let mut operation = existing_operation;
        operation["status"] = json!("CANCELING");
        if let Some(execution) = self.store.staged.bulk_query_executions.get_mut(&id) {
            execution.cancel_requested = true;
        }
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), operation.clone());
        let payload = json!({ "bulkOperation": operation, "userErrors": [] });
        ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
            "bulkOperationCancel",
            "bulk-operations",
            vec![id],
        ))
    }

    fn hydrate_bulk_operation_for_cancel(&mut self, request: &Request, id: &str) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "operationName": "BulkOperationHydrate",
                "query": BULK_OPERATION_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return;
        }
        let Some(operation) = response
            .body
            .get("data")
            .and_then(|data| data.get("bulkOperation"))
            .filter(|operation| operation.is_object())
            .cloned()
        else {
            return;
        };
        if operation.get("id").and_then(Value::as_str) == Some(id) {
            self.store
                .base
                .bulk_operations
                .insert(id.to_string(), operation);
        }
    }
}

/// Bulk mutation result rows are GraphQL-as-data in the same way as bulk query
/// JSONL. The nested compatibility executor intentionally returns canonical
/// resolver values, so this boundary must apply the selection from the mutation
/// document before persisting the result artifact.
fn bulk_project_mutation_response(
    mut response: Value,
    mutation_text: &str,
    variables: &Value,
    api_version: crate::admin_graphql::AdminApiVersion,
) -> Value {
    let variables = variables
        .as_object()
        .map(|variables| {
            variables
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
                .collect()
        })
        .unwrap_or_default();
    let variables =
        variables_with_operation_defaults(mutation_text, &variables, None).unwrap_or(variables);
    let Some(root) = parsed_document(mutation_text, &variables)
        .and_then(|document| document.root_fields.into_iter().next())
    else {
        return response;
    };
    let Some(data) = response.get_mut("data").and_then(Value::as_object_mut) else {
        return response;
    };
    let Some(value) = data
        .get(&root.response_key)
        .or_else(|| data.get(&root.name))
        .cloned()
    else {
        return response;
    };
    let value = bulk_project_value(&value, &root.selection, api_version);
    data.clear();
    data.insert(root.response_key, value);
    response
}

fn bulk_operation_record_value(spec: BulkOperationRecordSpec<'_>, artifact_url: String) -> Value {
    let terminal = bulk_operation_status_is_terminal(Some(spec.status));
    let has_result = matches!(spec.status, "COMPLETED" | "FAILED");
    let file_size_value = if has_result {
        json!(spec.file_size)
    } else {
        Value::Null
    };
    json!({
        "id": spec.id,
        "status": spec.status,
        "type": spec.operation_type,
        "errorCode": null,
        "createdAt": spec.created_at,
        "completedAt": if terminal { json!(spec.created_at) } else { Value::Null },
        "objectCount": if terminal { spec.count } else { "0" },
        "rootObjectCount": if terminal { spec.root_count } else { "0" },
        "fileSize": file_size_value,
        "url": if has_result { json!(artifact_url) } else { Value::Null },
        "partialDataUrl": null,
        "query": spec.query
    })
}

fn bulk_operation_run_query_user_errors(query_text: &str) -> Option<Vec<Value>> {
    if query_text.trim().is_empty() {
        return Some(vec![bulk_operation_run_query_user_error(
            "Invalid bulk query: syntax error, unexpected end of file",
        )]);
    }

    let Some(document) = parsed_document(query_text, &BTreeMap::new()) else {
        return Some(vec![bulk_operation_run_query_user_error(
            "Invalid bulk query: syntax error, unexpected end of file",
        )]);
    };
    if document.operation_type != OperationType::Query {
        return Some(vec![bulk_operation_run_query_user_error(
            "Invalid operation type. Only `query` operations are supported.",
        )]);
    }

    let analysis = BulkQueryAnalysis::analyze(&document.root_fields);
    let mut errors = Vec::new();
    if !analysis.nodes_connection_fields.is_empty() {
        let example_connection = &analysis.nodes_connection_fields[0];
        errors.push(bulk_operation_run_query_user_error(&format!(
            "All connection fields in a bulk query must select their contents using 'edges' > 'node', e.g: '{} {{ edges {{ node {{'. Selecting via 'nodes' is not supported. Invalid connection fields: '{}'.",
            example_connection,
            analysis.nodes_connection_fields.join("', '")
        )));
    }
    if analysis.has_top_level_node {
        errors.push(bulk_operation_run_query_user_error(
            "Bulk queries cannot contain a top level `node` field.",
        ));
    }
    if analysis.max_connection_depth > 2 {
        errors.push(bulk_operation_run_query_user_error(
            "Bulk queries cannot contain connections with a nesting depth greater than 2.",
        ));
    }
    if analysis.connection_count > 5 {
        errors.push(bulk_operation_run_query_user_error(
            "Bulk queries cannot contain more than 5 connections.",
        ));
    }
    if !analysis.nested_without_parent_id_fields.is_empty() {
        errors.push(bulk_operation_run_query_user_error(&format!(
            "The parent 'node' field for a nested connection must select the 'id' field without an alias and must be of 'ID' return type. Connection fields without 'id': {}.",
            analysis.nested_without_parent_id_fields.join(", ")
        )));
    }
    if analysis.has_connection_within_list {
        errors.push(bulk_operation_run_query_user_error(
            "Queries that contain a connection field within a list field are not currently supported.",
        ));
    }
    if analysis.connection_count == 0
        && (errors.is_empty() || (analysis.has_top_level_node && errors.len() == 1))
    {
        errors.push(bulk_operation_run_query_user_error(
            "Bulk queries must contain at least one connection.",
        ));
    }

    if errors.is_empty() {
        if let Some(user_error) = bulk_operation_query_storage_byte_limit_user_error(
            query_text,
            "Query is too large",
            Some("INVALID"),
        ) {
            return Some(vec![user_error]);
        }
        return None;
    }

    Some(errors)
}

fn bulk_operation_run_query_user_error(message: &str) -> Value {
    user_error(["query"], message, Some("INVALID"))
}

fn bulk_operation_query_storage_byte_limit_user_error(
    query_text: &str,
    message_prefix: &str,
    code: Option<&str>,
) -> Option<Value> {
    let byte_len = escaped_single_quoted_newlines_byte_len(query_text);
    if byte_len <= BULK_OPERATION_QUERY_STORAGE_BYTE_LIMIT {
        return None;
    }

    Some(user_error(
        ["query"],
        &format!(
            "{message_prefix} ({byte_len} bytes; maximum is {BULK_OPERATION_QUERY_STORAGE_BYTE_LIMIT} bytes)"
        ),
        code,
    ))
}

fn escaped_single_quoted_newlines_byte_len(query_text: &str) -> usize {
    let mut byte_len = 0;
    let mut index = 0;
    let mut inside_string = false;
    let mut inside_block_string = false;

    while index < query_text.len() {
        let remaining = &query_text[index..];
        if !inside_string && remaining.starts_with("\"\"\"") {
            inside_block_string = !inside_block_string;
            byte_len += 3;
            index += 3;
            continue;
        }
        if remaining.starts_with("\\\"") {
            byte_len += 2;
            index += 2;
            continue;
        }

        let Some(character) = remaining.chars().next() else {
            break;
        };
        match character {
            '"' if !inside_block_string => {
                inside_string = !inside_string;
                byte_len += 1;
                index += 1;
            }
            '\n' | '\r' if inside_string => {
                byte_len += 2;
                index += character.len_utf8();
            }
            _ => {
                byte_len += character.len_utf8();
                index += character.len_utf8();
            }
        }
    }

    byte_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn test_request(query: &str, variables: Value) -> Request {
        Request {
            method: "POST".to_string(),
            path: "/admin/api/2026-04/graphql.json".to_string(),
            headers: BTreeMap::new(),
            body: json!({ "query": query, "variables": variables }).to_string(),
        }
    }

    fn bulk_artifact_request(operation_id: &str) -> Request {
        Request {
            method: "GET".to_string(),
            path: bulk_operation_result_artifact_path(operation_id),
            headers: BTreeMap::new(),
            body: String::new(),
        }
    }

    fn meta_request(method: &str, path: &str, body: &str) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            headers: BTreeMap::new(),
            body: body.to_string(),
        }
    }

    fn observed_bulk_operation(id: &str, created_at: &str) -> Value {
        json!({
            "id": id,
            "status": "COMPLETED",
            "type": "QUERY",
            "errorCode": null,
            "createdAt": created_at,
            "completedAt": created_at,
            "objectCount": "4",
            "rootObjectCount": "2",
            "fileSize": "512",
            "url": "https://cdn.shopify.test/bulk/result.jsonl",
            "partialDataUrl": null,
            "query": "{ products { edges { node { id } } } }"
        })
    }

    fn upstream_bulk_operations_window(operation: Value) -> Value {
        json!({
            "data": {
                "bulkOperations": {
                    "edges": [{
                        "cursor": "bulk-operation-cursor",
                        "node": operation
                    }],
                    "nodes": [operation],
                    "pageInfo": {
                        "hasNextPage": false,
                        "hasPreviousPage": false,
                        "startCursor": "bulk-operation-cursor",
                        "endCursor": "bulk-operation-cursor"
                    }
                }
            }
        })
    }

    fn empty_bulk_products_catalog() -> Value {
        json!({
            "data": {
                "products": {
                    "nodes": [],
                    "pageInfo": {
                        "hasNextPage": false,
                        "endCursor": null
                    }
                }
            }
        })
    }

    fn seed_product(id: &str, title: &str, handle: &str) -> ProductRecord {
        ProductRecord {
            id: id.to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
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
            ..ProductRecord::default()
        }
    }

    fn staged_bulk_mutation_upload_path_with_body(
        proxy: &mut DraftProxy,
        filename: &str,
        body: &str,
    ) -> String {
        let staged = proxy.process_request(test_request(
            r#"
            mutation CreateBulkUpload($input: [StagedUploadInput!]!) {
              stagedUploadsCreate(input: $input) {
                stagedTargets { parameters { name value } }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "input": [{
                    "resource": "BULK_MUTATION_VARIABLES",
                    "filename": filename,
                    "mimeType": "text/jsonl",
                    "httpMethod": "POST",
                    "fileSize": body.len().to_string()
                }]
            }),
        ));
        assert_eq!(staged.status, 200);
        assert_eq!(
            staged.body["data"]["stagedUploadsCreate"]["userErrors"],
            json!([])
        );
        let path = staged.body["data"]["stagedUploadsCreate"]["stagedTargets"][0]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|parameter| parameter["name"] == "key")
            .and_then(|parameter| parameter["value"].as_str())
            .unwrap()
            .to_string();
        assert!(proxy.record_bulk_operation_staged_upload_body(&path, body.to_string()));
        path
    }

    fn test_proxy() -> DraftProxy {
        DraftProxy::new(Config {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_base_products(vec![
            seed_product("gid://shopify/Product/1", "Red product", "red-product"),
            seed_product("gid://shopify/Product/2", "Blue product", "blue-product"),
        ])
        .with_upstream_transport(|_| panic!("bulk operation tests should not call upstream"))
    }

    fn read_bulk_operation(proxy: &mut DraftProxy, id: &str) -> Response {
        proxy.process_request(test_request(
            r#"
            query PollBulkOperation($id: ID!) {
              bulkOperation(id: $id) {
                id
                status
                errorCode
                completedAt
                objectCount
                rootObjectCount
                fileSize
                url
              }
            }
            "#,
            json!({ "id": id }),
        ))
    }

    #[test]
    fn bulk_query_submission_is_constant_and_execution_has_a_fixed_page_budget_per_poll() {
        let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured_calls = Arc::clone(&upstream_calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body is JSON");
            captured_calls.lock().unwrap().push(body.clone());
            let page = match body.pointer("/variables/after") {
                Some(Value::Null) => 0,
                Some(Value::String(after)) => after
                    .strip_prefix("page-")
                    .and_then(|page| page.parse::<usize>().ok())
                    .unwrap_or_else(|| panic!("unexpected catalog cursor: {after}")),
                after => panic!("unexpected catalog cursor: {after:?}"),
            };
            let has_next_page = page < BULK_CATALOG_PAGES_PER_READ;
            let end_cursor = format!("page-{}", page + 1);
            let page_size = usize::try_from(BULK_CATALOG_PAGE_SIZE).unwrap();
            let nodes = (0..page_size)
                .map(|index| {
                    let index = page * page_size + index;
                    json!({
                        "id": format!("gid://shopify/Product/{index}"),
                        "title": format!("Catalog product {index}")
                    })
                })
                .collect::<Vec<_>>();
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({
                    "data": {
                        "products": {
                            "nodes": nodes,
                            "pageInfo": {
                                "hasNextPage": has_next_page,
                                "endCursor": end_cursor
                            }
                        }
                    }
                }),
            }
        });

        let run = proxy.process_request(test_request(
            r#"
            mutation SubmitPagedBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status completedAt objectCount url }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "query": "{ products { edges { node { id title } } } }" }),
        ));
        assert_eq!(
            run.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let created = &run.body["data"]["bulkOperationRunQuery"]["bulkOperation"];
        let created_state_version = run.headers["x-sdp-state-version"].clone();
        assert_eq!(created["status"], json!("CREATED"));
        assert_eq!(created["completedAt"], Value::Null);
        assert_eq!(created["objectCount"], json!("0"));
        assert_eq!(created["url"], Value::Null);
        assert_eq!(
            upstream_calls.lock().unwrap().len(),
            0,
            "admission must not hydrate any catalog page"
        );
        let operation_id = created["id"].as_str().unwrap().to_string();
        assert_eq!(
            proxy
                .process_request(bulk_artifact_request(&operation_id))
                .status,
            404
        );

        let running = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            running.body["data"]["bulkOperation"]["status"],
            json!("RUNNING")
        );
        assert_eq!(
            upstream_calls.lock().unwrap().len(),
            BULK_CATALOG_PAGES_PER_READ,
            "one poll must never exceed the fixed root-page execution budget"
        );
        assert_ne!(
            running.headers["x-sdp-state-version"],
            created_state_version
        );

        let completed = read_bulk_operation(&mut proxy, &operation_id);
        let completed = &completed.body["data"]["bulkOperation"];
        assert_eq!(completed["status"], json!("COMPLETED"));
        let expected_count =
            (BULK_CATALOG_PAGES_PER_READ + 1) * usize::try_from(BULK_CATALOG_PAGE_SIZE).unwrap();
        assert_eq!(completed["objectCount"], json!(expected_count.to_string()));
        assert_eq!(
            completed["rootObjectCount"],
            json!(expected_count.to_string())
        );
        assert_eq!(
            upstream_calls.lock().unwrap().len(),
            BULK_CATALOG_PAGES_PER_READ + 1
        );
        let artifact = proxy.process_request(bulk_artifact_request(&operation_id));
        assert_eq!(artifact.status, 200);
        assert_eq!(
            artifact.body.as_str().unwrap().lines().count(),
            expected_count
        );
    }

    #[test]
    fn bulk_query_nested_overflow_fails_without_per_row_hydration() {
        let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured_calls = Arc::clone(&upstream_calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body is JSON");
            captured_calls.lock().unwrap().push(body.clone());
            assert_eq!(body["operationName"], json!("BulkProductsCatalogHydrate"));
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({
                    "data": {
                        "products": {
                            "nodes": [{
                                "id": "gid://shopify/Product/nested",
                                "title": "Nested overflow",
                                "bulkNested0": {
                                    "nodes": [{
                                        "id": "gid://shopify/Metafield/first",
                                        "namespace": "custom",
                                        "key": "first",
                                        "value": "one"
                                    }],
                                    "pageInfo": {
                                        "hasNextPage": true,
                                        "endCursor": "nested-page-1"
                                    }
                                }
                            }],
                            "pageInfo": {
                                "hasNextPage": false,
                                "endCursor": "product-page-1"
                            }
                        }
                    }
                }),
            }
        });

        let run = proxy.process_request(test_request(
            r#"
            mutation SubmitNestedBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"{
                  products {
                    edges {
                      node {
                        id
                        title
                        metafields(namespace: "custom") {
                          edges { node { id namespace key value } }
                        }
                      }
                    }
                  }
                }"#
            }),
        ));
        assert_eq!(upstream_calls.lock().unwrap().len(), 0);
        let operation_id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let failed = read_bulk_operation(&mut proxy, &operation_id);
        let failed = &failed.body["data"]["bulkOperation"];
        assert_eq!(failed["status"], json!("FAILED"));
        assert_eq!(failed["errorCode"], json!("INTERNAL_SERVER_ERROR"));
        assert_eq!(failed["url"], Value::Null);
        assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn pending_bulk_query_can_fail_cancel_and_discard_without_result_state() {
        let mut failing_proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(|_| Response {
            status: 503,
            headers: BTreeMap::new(),
            body: json!({ "errors": [{ "message": "Shopify unavailable" }] }),
        });
        let run = failing_proxy.process_request(test_request(
            "mutation Submit($query: String!) { bulkOperationRunQuery(query: $query) { bulkOperation { id status } userErrors { field message code } } }",
            json!({ "query": "{ products { edges { node { id } } } }" }),
        ));
        let failing_id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let failed = read_bulk_operation(&mut failing_proxy, &failing_id);
        assert_eq!(
            failed.body["data"]["bulkOperation"]["status"],
            json!("FAILED")
        );
        assert_eq!(failed.body["data"]["bulkOperation"]["url"], Value::Null);

        let mut proxy = test_proxy();
        let run = proxy.process_request(test_request(
            "mutation Submit($query: String!) { bulkOperationRunQuery(query: $query) { bulkOperation { id status } userErrors { field message code } } }",
            json!({ "query": "{ products { edges { node { id title } } } }" }),
        ));
        let operation_id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let cancel = proxy.process_request(test_request(
            "mutation Cancel($id: ID!) { bulkOperationCancel(id: $id) { bulkOperation { id status } userErrors { field message } } }",
            json!({ "id": operation_id }),
        ));
        assert_eq!(
            cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["status"],
            json!("CANCELING")
        );
        let canceled = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            canceled.body["data"]["bulkOperation"]["status"],
            json!("CANCELED")
        );
        assert_eq!(
            proxy
                .process_request(bulk_artifact_request(&operation_id))
                .status,
            404
        );

        let reset = proxy.process_request(meta_request("POST", "/__meta/reset", ""));
        assert_eq!(reset.status, 200);
        let discarded = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(discarded.body["data"]["bulkOperation"], Value::Null);
        let state = proxy.process_request(meta_request("GET", "/__meta/state", ""));
        assert!(state.body["stagedState"]["bulkOperations"]
            .as_object()
            .is_none_or(serde_json::Map::is_empty));
        assert!(state.body["stagedState"]["bulkOperationResults"]
            .as_object()
            .is_none_or(serde_json::Map::is_empty));
    }

    #[test]
    fn pending_bulk_query_execution_round_trips_dump_restore() {
        let mut proxy = test_proxy();
        let run = proxy.process_request(test_request(
            "mutation Submit($query: String!) { bulkOperationRunQuery(query: $query) { bulkOperation { id status } userErrors { field message } } }",
            json!({ "query": "{ products { edges { node { id title } } } }" }),
        ));
        let operation_id = run.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let dump = proxy.process_request(meta_request("POST", "/__meta/dump", ""));
        assert_eq!(
            dump.body["state"]["stagedState"]["bulkQueryExecutions"][operation_id.as_str()]
                ["after"],
            Value::Null
        );

        let mut restored = DraftProxy::new(Config {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(|_| panic!("restored Snapshot execution must stay local"));
        let restore = restored.process_request(meta_request(
            "POST",
            "/__meta/restore",
            &dump.body.to_string(),
        ));
        assert_eq!(restore.status, 200);
        let completed = read_bulk_operation(&mut restored, &operation_id);
        assert_eq!(
            completed.body["data"]["bulkOperation"]["status"],
            json!("COMPLETED")
        );
        assert_eq!(
            restored
                .process_request(bulk_artifact_request(&operation_id))
                .status,
            200
        );
    }

    #[test]
    fn product_bulk_query_stays_local_and_logs_original_mutation() {
        let upstream_calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_transport = Arc::clone(&upstream_calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body is JSON");
            assert_eq!(
                body["operationName"],
                json!("BulkProductsCatalogHydrate"),
                "supported bulk export may only issue its read-side catalog hydration"
            );
            let query = body["query"].as_str().expect("hydrate query text");
            assert!(query.starts_with("query BulkProductsCatalogHydrate"));
            assert!(!query.contains("bulkOperationRunQuery"));
            calls_for_transport.lock().unwrap().push(request);
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({
                    "data": {
                        "products": {
                            "nodes": [{
                                "id": "gid://shopify/Product/base",
                                "title": "Base product"
                            }],
                            "pageInfo": {
                                "hasNextPage": false,
                                "endCursor": null
                            }
                        }
                    }
                }),
            }
        });
        let create = proxy.process_request(test_request(
            r#"
            mutation StageProductForBulkExport($product: ProductCreateInput!) {
              productCreate(product: $product) {
                product { id title }
                userErrors { field message }
              }
            }
            "#,
            json!({ "product": { "title": "Staged product" } }),
        ));
        assert_eq!(create.status, 200);
        assert_eq!(
            create.body["data"]["productCreate"]["userErrors"],
            json!([])
        );
        let staged_product_id = create.body["data"]["productCreate"]["product"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let raw_mutation = r#"
            mutation StageLocalBulkExport($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status type }
                userErrors { field message code }
              }
            }
        "#;

        let run_request = test_request(
            raw_mutation,
            json!({ "query": "{ products { edges { node { id title } } } }" }),
        );
        let expected_raw_body = run_request.body.clone();
        let response = proxy.process_request(run_request);

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        assert_eq!(
            upstream_calls.lock().unwrap().len(),
            0,
            "submission must not hydrate the catalog"
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let completed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            completed.body["data"]["bulkOperation"]["status"],
            json!("COMPLETED")
        );
        assert_eq!(
            upstream_calls.lock().unwrap().len(),
            1,
            "the first poll executes one catalog page"
        );
        let artifact = proxy.process_request(bulk_artifact_request(&operation_id));
        assert_eq!(artifact.status, 200);
        let rows = artifact
            .body
            .as_str()
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(rows
            .iter()
            .any(|row| row["id"] == "gid://shopify/Product/base"));
        assert!(rows.iter().any(|row| row["id"] == staged_product_id));
        let log = proxy.process_request(meta_request("GET", "/__meta/log", ""));
        assert_eq!(log.body["entries"].as_array().unwrap().len(), 2);
        assert_eq!(log.body["entries"][1]["rawBody"], expected_raw_body);
    }

    #[test]
    fn unsupported_non_product_bulk_query_never_calls_upstream() {
        let upstream_calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_transport = Arc::clone(&upstream_calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            calls_for_transport.lock().unwrap().push(request);
            Response {
                status: 500,
                headers: BTreeMap::new(),
                body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
            }
        });

        let response = proxy.process_request(test_request(
            r#"
            mutation RejectUnmodeledBulkExport($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status type }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "query": "{ orders { edges { node { id } } } }" }),
        ));

        assert_eq!(response.status, 200);
        assert!(
            upstream_calls.lock().unwrap().is_empty(),
            "unsupported bulk query must not run a substitute mutation upstream"
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["bulkOperation"],
            Value::Null
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([{
                "field": ["query"],
                "message": "Bulk query root `orders` is accepted by Shopify's schema-driven validator but is not yet supported by the local JSONL synthesizer.",
                "code": null
            }])
        );
        let log = proxy.process_request(meta_request("GET", "/__meta/log", ""));
        assert_eq!(log.body["entries"], json!([]));
    }

    fn instrumented_live_proxy() -> (DraftProxy, Arc<Mutex<Vec<Request>>>) {
        let upstream_calls = Arc::new(Mutex::new(Vec::<Request>::new()));
        let upstream_calls_for_transport = Arc::clone(&upstream_calls);
        let proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: Some(UnsupportedMutationMode::Passthrough),
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            upstream_calls_for_transport.lock().unwrap().push(request);
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({ "data": {} }),
            }
        });
        (proxy, upstream_calls)
    }

    fn run_bulk_mutation_import(
        proxy: &mut DraftProxy,
        mutation: &str,
        staged_upload_path: &str,
    ) -> Response {
        proxy.process_request(test_request(
            r#"
            mutation RunBulkImport($mutation: String!, $path: String!) {
              bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
                bulkOperation { id status type }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "mutation": mutation, "path": staged_upload_path }),
        ))
    }

    fn create_variant(proxy: &mut DraftProxy, product_id: &str, sku: &str) -> Value {
        let response = proxy.process_request(test_request(
            r#"
            mutation CreateVariantForBulkTest($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
              productVariantsBulkCreate(productId: $productId, variants: $variants) {
                productVariants { id sku }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "productId": product_id,
                "variants": [{
                    "inventoryItem": { "sku": sku },
                    "price": "10.00"
                }]
            }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["productVariantsBulkCreate"]["userErrors"],
            json!([])
        );
        response.body["data"]["productVariantsBulkCreate"]["productVariants"][0].clone()
    }

    #[test]
    fn escaped_single_quoted_newlines_byte_len_counts_escaped_regular_string_newlines() {
        assert_eq!(escaped_single_quoted_newlines_byte_len("\"a\nb\""), 6);
        assert_eq!(escaped_single_quoted_newlines_byte_len("\"a\rb\""), 6);
        assert_eq!(escaped_single_quoted_newlines_byte_len("\"é\""), 4);
    }

    #[test]
    fn escaped_single_quoted_newlines_byte_len_preserves_block_string_newlines() {
        assert_eq!(
            escaped_single_quoted_newlines_byte_len("\"\"\"a\nb\"\"\""),
            9
        );
    }

    #[test]
    fn completed_bulk_operation_record_uses_configured_artifact_url_builder() {
        let proxy = DraftProxy::new(Config {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: None,
            port: 3123,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        });
        let operation = proxy.bulk_operation_record(BulkOperationRecordSpec {
            id: "gid://shopify/BulkOperation/123",
            status: "COMPLETED",
            operation_type: "QUERY",
            query: "{ products { edges { node { id } } } }",
            count: "1",
            root_count: "1",
            created_at: "2024-01-01T00:00:00Z",
            file_size: "10",
        });
        assert_eq!(
            operation["url"],
            json!("https://localhost:3123/__meta/bulk-operations/123/result.jsonl")
        );
    }

    #[test]
    fn cold_bulk_operations_without_sort_key_forwards_upstream() {
        let upstream_calls = Arc::new(Mutex::new(Vec::new()));
        let upstream_calls_for_transport = Arc::clone(&upstream_calls);
        let real_operation = observed_bulk_operation(
            "gid://shopify/BulkOperation/7749092278578",
            "2026-01-02T00:00:00Z",
        );
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            upstream_calls_for_transport
                .lock()
                .unwrap()
                .push(request.body.clone());
            let body: Value = serde_json::from_str(&request.body).unwrap();
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: if body["operationName"] == "BulkProductsCatalogHydrate" {
                    empty_bulk_products_catalog()
                } else {
                    upstream_bulk_operations_window(real_operation.clone())
                },
            }
        });

        let response = proxy.process_request(test_request(
            r#"
            query ColdBulkOperationsNoSortKey {
              bulkOperations(first: 10) {
                edges { cursor node { id status type createdAt } }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            "#,
            json!({}),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(upstream_calls.lock().unwrap().len(), 1);
        assert_eq!(
            response.body["data"]["bulkOperations"]["edges"][0]["node"]["id"],
            json!("gid://shopify/BulkOperation/7749092278578")
        );
    }

    #[test]
    fn cold_bulk_operation_by_id_forwards_upstream() {
        let real_id = "gid://shopify/BulkOperation/7749063934258";
        let upstream_calls = Arc::new(Mutex::new(Vec::new()));
        let upstream_calls_for_transport = Arc::clone(&upstream_calls);
        let real_operation = observed_bulk_operation(real_id, "2026-01-03T00:00:00Z");
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            upstream_calls_for_transport
                .lock()
                .unwrap()
                .push(request.body.clone());
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({
                    "data": {
                        "bulkOperation": real_operation
                    }
                }),
            }
        });

        let response = proxy.process_request(test_request(
            r#"
            query ColdBulkOperationById($id: ID!) {
              bulkOperation(id: $id) {
                id
                status
                type
                createdAt
              }
            }
            "#,
            json!({ "id": real_id }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(upstream_calls.lock().unwrap().len(), 1);
        assert_eq!(response.body["data"]["bulkOperation"]["id"], json!(real_id));
    }

    #[test]
    fn observed_real_bulk_operation_stays_visible_with_staged_operation() {
        let real_id = "gid://shopify/BulkOperation/7749099127090";
        let upstream_calls = Arc::new(Mutex::new(Vec::new()));
        let upstream_calls_for_transport = Arc::clone(&upstream_calls);
        let real_operation = observed_bulk_operation(real_id, "2026-01-01T00:00:00Z");
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_base_products(vec![seed_product(
            "gid://shopify/Product/1",
            "Observed bulk product",
            "observed-bulk-product",
        )])
        .with_upstream_transport(move |request| {
            upstream_calls_for_transport
                .lock()
                .unwrap()
                .push(request.body.clone());
            let body: Value = serde_json::from_str(&request.body).unwrap();
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: if body["operationName"] == "BulkProductsCatalogHydrate" {
                    empty_bulk_products_catalog()
                } else {
                    upstream_bulk_operations_window(real_operation.clone())
                },
            }
        });

        let observed = proxy.process_request(test_request(
            r#"
            query ObserveBulkOperationsWindow {
              bulkOperations(first: 10) {
                edges {
                  node {
                    id
                    status
                    type
                    createdAt
                    completedAt
                    objectCount
                    rootObjectCount
                    fileSize
                    url
                    partialDataUrl
                    query
                  }
                }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(observed.status, 200);
        assert_eq!(
            observed.body["data"]["bulkOperations"]["edges"][0]["node"]["id"],
            json!(real_id)
        );

        let staged = proxy.process_request(test_request(
            r#"
            mutation StageBulkOperation($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status type }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": "{ products { edges { node { id } } } }"
            }),
        ));
        assert_eq!(staged.status, 200);
        assert_eq!(
            staged.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let staged_id = staged.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let combined = proxy.process_request(test_request(
            r#"
            query CombinedBulkOperations {
              bulkOperations(first: 10, sortKey: CREATED_AT) {
                edges { node { id status type createdAt } }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(combined.status, 200);
        let ids = combined.body["data"]["bulkOperations"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| edge["node"]["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(ids.contains(&real_id.to_string()), "combined ids: {ids:?}");
        assert!(ids.contains(&staged_id), "combined ids: {ids:?}");
        assert_eq!(upstream_calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn staged_bulk_operation_hydrates_real_window_before_overlay() {
        let real_id = "gid://shopify/BulkOperation/7745508180274";
        let upstream_calls = Arc::new(Mutex::new(Vec::new()));
        let upstream_calls_for_transport = Arc::clone(&upstream_calls);
        let real_operation = observed_bulk_operation(real_id, "2026-01-01T00:00:00Z");
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_base_products(vec![seed_product(
            "gid://shopify/Product/1",
            "Staged first product",
            "staged-first-product",
        )])
        .with_upstream_transport(move |request| {
            upstream_calls_for_transport
                .lock()
                .unwrap()
                .push(request.body.clone());
            let body: Value = serde_json::from_str(&request.body).unwrap();
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: if body["operationName"] == "BulkProductsCatalogHydrate" {
                    empty_bulk_products_catalog()
                } else {
                    upstream_bulk_operations_window(real_operation.clone())
                },
            }
        });

        let staged = proxy.process_request(test_request(
            r#"
            mutation StageBulkOperationBeforeObservation($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status type }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": "{ products { edges { node { id } } } }"
            }),
        ));
        assert_eq!(staged.status, 200);
        let staged_id = staged.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let combined = proxy.process_request(test_request(
            r#"
            query StagedFirstCombinedBulkOperations {
              bulkOperations(first: 10, sortKey: CREATED_AT) {
                edges { node { id status type createdAt } }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(combined.status, 200);
        let ids = combined.body["data"]["bulkOperations"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| edge["node"]["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(ids.contains(&real_id.to_string()), "combined ids: {ids:?}");
        assert!(ids.contains(&staged_id), "combined ids: {ids:?}");
        assert_eq!(upstream_calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn staged_bulk_operation_wins_over_observed_operation_by_id() {
        let real_id = "gid://shopify/BulkOperation/7964059697458";
        let mut running_operation = observed_bulk_operation(real_id, "2026-01-01T00:00:00Z");
        running_operation["status"] = json!("RUNNING");
        running_operation["completedAt"] = Value::Null;
        running_operation["objectCount"] = json!("0");
        running_operation["rootObjectCount"] = json!("0");
        running_operation["fileSize"] = Value::Null;
        running_operation["url"] = Value::Null;
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |_| Response {
            status: 200,
            headers: BTreeMap::new(),
            body: json!({
                "data": {
                    "bulkOperation": running_operation
                }
            }),
        });

        let observed = proxy.process_request(test_request(
            r#"
            query ObserveRunningBulkOperation($id: ID!) {
              bulkOperation(id: $id) {
                id
                status
                type
                createdAt
                completedAt
                objectCount
                rootObjectCount
                fileSize
                url
                partialDataUrl
                query
              }
            }
            "#,
            json!({ "id": real_id }),
        ));
        assert_eq!(observed.status, 200);
        assert_eq!(
            observed.body["data"]["bulkOperation"]["status"],
            json!("RUNNING")
        );

        let cancel = proxy.process_request(test_request(
            r#"
            mutation CancelObservedBulkOperation($id: ID!) {
              bulkOperationCancel(id: $id) {
                bulkOperation { id status }
                userErrors { field message }
              }
            }
            "#,
            json!({ "id": real_id }),
        ));
        assert_eq!(cancel.status, 200);
        assert_eq!(
            cancel.body["data"]["bulkOperationCancel"]["bulkOperation"]["status"],
            json!("CANCELING")
        );

        let read = proxy.process_request(test_request(
            r#"
            query ReadCanceledBulkOperation($id: ID!) {
              bulkOperation(id: $id) {
                id
                status
              }
              currentBulkOperation(type: QUERY) {
                id
                status
              }
            }
            "#,
            json!({ "id": real_id }),
        ));
        assert_eq!(read.status, 200);
        assert_eq!(
            read.body["data"]["bulkOperation"]["status"],
            json!("CANCELING")
        );
        assert_eq!(
            read.body["data"]["currentBulkOperation"]["status"],
            json!("CANCELING")
        );
    }

    #[test]
    fn observed_bulk_operation_base_state_round_trips_dump_restore() {
        let real_id = "gid://shopify/BulkOperation/7749092278578";
        let real_operation = observed_bulk_operation(real_id, "2026-01-01T00:00:00Z");
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |_| Response {
            status: 200,
            headers: BTreeMap::new(),
            body: upstream_bulk_operations_window(real_operation.clone()),
        });

        let observed = proxy.process_request(test_request(
            r#"
            query ObserveBulkOperationsForRestore {
              bulkOperations(first: 10) {
                edges {
                  node {
                    id
                    status
                    type
                    createdAt
                    completedAt
                    objectCount
                    rootObjectCount
                    fileSize
                    url
                    partialDataUrl
                    query
                  }
                }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(observed.status, 200);

        let dump = proxy.process_request(meta_request("POST", "/__meta/dump", ""));
        assert_eq!(dump.status, 200);
        assert_eq!(
            dump.body["state"]["baseState"]["bulkOperations"][real_id]["id"],
            json!(real_id)
        );
        assert_eq!(
            dump.body["state"]["baseState"]["bulkOperationOrder"],
            json!([real_id])
        );
        assert_eq!(
            dump.body["state"]["baseState"]["bulkOperationsObserved"],
            json!(true)
        );

        let mut restored = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(|_| panic!("restored bulk operation read should stay local"));
        let restore = restored.process_request(meta_request(
            "POST",
            "/__meta/restore",
            &dump.body.to_string(),
        ));
        assert_eq!(restore.status, 200);

        let read = restored.process_request(test_request(
            r#"
            query RestoredBulkOperations {
              bulkOperations(first: 10) {
                edges { node { id status type createdAt } }
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(read.status, 200);
        assert_eq!(
            read.body["data"]["bulkOperations"]["edges"][0]["node"]["id"],
            json!(real_id)
        );
    }

    #[test]
    fn product_variants_bulk_query_jsonl_applies_query_filter() {
        let mut proxy = test_proxy();
        let red = create_variant(&mut proxy, "gid://shopify/Product/1", "RED-SKU");
        create_variant(&mut proxy, "gid://shopify/Product/2", "BLUE-SKU");

        let response = proxy.process_request(test_request(
            r#"
            mutation RunVariantBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status objectCount }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  productVariants(query: "sku:RED-SKU") {
                    edges {
                      node { id sku }
                    }
                  }
                }
                "#
            }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let completed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            completed.body["data"]["bulkOperation"]["status"],
            json!("COMPLETED")
        );
        let artifact = proxy.process_request(bulk_artifact_request(&operation_id));
        assert_eq!(artifact.status, 200);
        let rows = artifact
            .body
            .as_str()
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(rows, vec![json!({ "id": red["id"], "sku": "RED-SKU" })]);
    }

    #[test]
    fn product_variants_bulk_query_hydrates_every_upstream_page() {
        let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured_calls = Arc::clone(&upstream_calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body is JSON");
            captured_calls.lock().unwrap().push(body.clone());
            assert_eq!(
                body["operationName"],
                json!("BulkProductVariantsCatalogHydrate")
            );
            let query = body["query"].as_str().expect("hydrate query text");
            assert!(query.contains("sku"));
            assert!(query.contains("product { id title }"));
            assert!(!query.contains("barcode"));
            assert!(!query.contains("bulkOperationRunQuery"));

            let (variant_id, sku, product_id, product_title, has_next_page, end_cursor) =
                match body.pointer("/variables/after") {
                    Some(Value::Null) => (
                        "gid://shopify/ProductVariant/101",
                        "FIRST-SKU",
                        "gid://shopify/Product/11",
                        "First product",
                        true,
                        "variant-page-1",
                    ),
                    Some(Value::String(after)) if after == "variant-page-1" => (
                        "gid://shopify/ProductVariant/202",
                        "SECOND-SKU",
                        "gid://shopify/Product/22",
                        "Second product",
                        false,
                        "variant-page-2",
                    ),
                    after => panic!("unexpected variant catalog cursor: {after:?}"),
                };
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({
                    "data": {
                        "productVariants": {
                            "nodes": [{
                                "id": variant_id,
                                "sku": sku,
                                "product": {
                                    "id": product_id,
                                    "title": product_title
                                }
                            }],
                            "pageInfo": {
                                "hasNextPage": has_next_page,
                                "endCursor": end_cursor
                            }
                        }
                    }
                }),
            }
        });

        let response = proxy.process_request(test_request(
            r#"
            mutation RunHydratedVariantBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  productVariants {
                    edges {
                      node { id sku product { id title } }
                    }
                  }
                }
                "#
            }),
        ));

        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let completed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            completed.body["data"]["bulkOperation"]["status"],
            json!("COMPLETED")
        );
        let artifact = proxy.process_request(bulk_artifact_request(&operation_id));
        let rows = artifact
            .body
            .as_str()
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            vec![
                json!({
                    "id": "gid://shopify/ProductVariant/101",
                    "sku": "FIRST-SKU",
                    "product": {
                        "id": "gid://shopify/Product/11",
                        "title": "First product"
                    }
                }),
                json!({
                    "id": "gid://shopify/ProductVariant/202",
                    "sku": "SECOND-SKU",
                    "product": {
                        "id": "gid://shopify/Product/22",
                        "title": "Second product"
                    }
                })
            ]
        );
        assert_eq!(upstream_calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn product_variant_bulk_query_fails_nested_overflow_without_per_row_requests() {
        let variant_id = "gid://shopify/ProductVariant/909";
        let product_id = "gid://shopify/Product/808";
        let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured_calls = Arc::clone(&upstream_calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body is JSON");
            captured_calls.lock().unwrap().push(body.clone());
            match body["operationName"].as_str() {
                Some("BulkProductVariantsCatalogHydrate") => {
                    let query = body["query"].as_str().expect("catalog hydrate query");
                    assert!(query.contains("bulkNested0: metafields("));
                    assert!(query.contains("namespace: \"custom\""));
                    Response {
                        status: 200,
                        headers: BTreeMap::new(),
                        body: json!({
                            "data": {
                                "productVariants": {
                                    "nodes": [{
                                        "id": variant_id,
                                        "sku": "NESTED-VARIANT",
                                        "product": {
                                            "id": product_id,
                                            "title": "Nested variant product"
                                        },
                                        "bulkNested0": {
                                            "nodes": [{
                                                "id": "gid://shopify/Metafield/901",
                                                "namespace": "custom",
                                                "key": "first",
                                                "value": "one"
                                            }],
                                            "pageInfo": {
                                                "hasNextPage": true,
                                                "endCursor": "variant-metafield-page-1"
                                            }
                                        }
                                    }],
                                    "pageInfo": {
                                        "hasNextPage": false,
                                        "endCursor": "variant-page-1"
                                    }
                                }
                            }
                        }),
                    }
                }
                Some("BulkProductVariantNestedCatalogHydrate") => {
                    assert_eq!(body["variables"]["id"], json!(variant_id));
                    assert_eq!(
                        body["variables"]["after"],
                        json!("variant-metafield-page-1")
                    );
                    let query = body["query"].as_str().expect("nested hydrate query");
                    assert!(query.contains("productVariant(id: $id)"));
                    assert!(query.contains("bulkNested0: metafields("));
                    Response {
                        status: 200,
                        headers: BTreeMap::new(),
                        body: json!({
                            "data": {
                                "productVariant": {
                                    "bulkNested0": {
                                        "nodes": [{
                                            "id": "gid://shopify/Metafield/902",
                                            "namespace": "custom",
                                            "key": "second",
                                            "value": "two"
                                        }],
                                        "pageInfo": {
                                            "hasNextPage": false,
                                            "endCursor": "variant-metafield-page-2"
                                        }
                                    }
                                }
                            }
                        }),
                    }
                }
                operation => panic!("unexpected operation: {operation:?}"),
            }
        });

        let response = proxy.process_request(test_request(
            r#"
            mutation RunNestedHydratedVariantBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  productVariants {
                    edges {
                      node {
                        id
                        sku
                        metafields(namespace: "custom") {
                          edges { node { id namespace key value } }
                        }
                      }
                    }
                  }
                }
                "#
            }),
        ));

        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let failed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            failed.body["data"]["bulkOperation"]["status"],
            json!("FAILED")
        );
        assert_eq!(
            proxy
                .process_request(bulk_artifact_request(&operation_id))
                .status,
            404
        );
        assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn products_bulk_query_hydrates_every_upstream_page_before_applying_staged_overlays() {
        let updated_id = "gid://shopify/Product/100";
        let deleted_id = "gid://shopify/Product/200";
        let untouched_id = "gid://shopify/Product/300";
        let mut updated = seed_product(updated_id, "Observed before update", "observed-update");
        updated.tags = vec!["bulk-export".to_string()];
        let mut deleted = seed_product(deleted_id, "Observed before delete", "observed-delete");
        deleted.tags = vec!["bulk-export".to_string()];

        let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured_calls = Arc::clone(&upstream_calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_base_products(vec![updated, deleted])
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body is JSON");
            captured_calls.lock().unwrap().push(body.clone());
            assert_eq!(request.path, "/admin/api/2026-04/graphql.json");
            assert_eq!(body["operationName"], json!("BulkProductsCatalogHydrate"));
            let query = body["query"].as_str().expect("hydrate query text");
            assert!(query.starts_with("query BulkProductsCatalogHydrate"));
            assert!(query.contains("tags"), "tag search needs hydrated tags: {query}");
            assert!(query.contains("title"), "selected title must be hydrated: {query}");
            assert!(
                !query.contains("descriptionHtml") && !query.contains("vendor"),
                "hydrate should retain selected/filter fields without an unrelated broad product document: {query}"
            );
            assert!(
                !query.contains("bulkOperationRunQuery"),
                "supported bulk export must only issue query hydration"
            );

            match body.pointer("/variables/after") {
                Some(Value::Null) => Response {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: json!({
                        "data": {
                            "products": {
                                "nodes": [{
                                    "id": updated_id,
                                    "title": "Upstream title",
                                    "tags": ["bulk-export"]
                                }],
                                "pageInfo": {
                                    "hasNextPage": true,
                                    "endCursor": "cursor-page-1"
                                }
                            }
                        }
                    }),
                },
                Some(Value::String(after)) if after == "cursor-page-1" => Response {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: json!({
                        "data": {
                            "products": {
                                "nodes": [
                                    {
                                        "id": deleted_id,
                                        "title": "Deleted upstream title",
                                        "tags": ["bulk-export"]
                                    },
                                    {
                                        "id": untouched_id,
                                        "title": "Untouched upstream title",
                                        "tags": ["bulk-export"]
                                    }
                                ],
                                "pageInfo": {
                                    "hasNextPage": false,
                                    "endCursor": "cursor-page-2"
                                }
                            }
                        }
                    }),
                },
                after => panic!("unexpected catalog hydrate cursor: {after:?}"),
            }
        });

        let update = proxy.process_request(test_request(
            r#"
            mutation UpdateBulkOverlayProduct($product: ProductUpdateInput!) {
              productUpdate(product: $product) {
                product { id title }
                userErrors { field message }
              }
            }
            "#,
            json!({ "product": { "id": updated_id, "title": "Locally updated title" } }),
        ));
        assert_eq!(
            update.body["data"]["productUpdate"]["userErrors"],
            json!([])
        );

        let delete = proxy.process_request(test_request(
            r#"
            mutation DeleteBulkOverlayProduct($input: ProductDeleteInput!) {
              productDelete(input: $input) {
                deletedProductId
                userErrors { field message }
              }
            }
            "#,
            json!({ "input": { "id": deleted_id } }),
        ));
        assert_eq!(
            delete.body["data"]["productDelete"]["deletedProductId"],
            json!(deleted_id)
        );

        let create = proxy.process_request(test_request(
            r#"
            mutation CreateBulkOverlayProduct($product: ProductCreateInput!) {
              productCreate(product: $product) {
                product { id title }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "product": {
                    "title": "Locally created title",
                    "tags": ["bulk-export"]
                }
            }),
        ));
        assert_eq!(
            create.body["data"]["productCreate"]["userErrors"],
            json!([])
        );
        let created_id = create.body["data"]["productCreate"]["product"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let response = proxy.process_request(test_request(
            r#"
            mutation RunHydratedProductBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  products(query: "tag:bulk-export") {
                    edges { node { id title } }
                  }
                }
                "#
            }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let completed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            completed.body["data"]["bulkOperation"]["status"],
            json!("COMPLETED")
        );
        let artifact = proxy.process_request(bulk_artifact_request(&operation_id));
        let rows = artifact
            .body
            .as_str()
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            rows,
            vec![
                json!({ "id": updated_id, "title": "Locally updated title" }),
                json!({ "id": untouched_id, "title": "Untouched upstream title" }),
                json!({ "id": created_id, "title": "Locally created title" })
            ]
        );
        let calls = upstream_calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            2,
            "catalog hydration must stop on the complete page"
        );
        assert!(calls.iter().all(|call| {
            call["query"]
                .as_str()
                .is_some_and(|query| query.trim_start().starts_with("query "))
        }));
    }

    #[test]
    fn product_bulk_query_fails_nested_overflow_without_per_row_requests() {
        let product_id = "gid://shopify/Product/707";
        let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured_calls = Arc::clone(&upstream_calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body is JSON");
            captured_calls.lock().unwrap().push(body.clone());
            match body["operationName"].as_str() {
                Some("BulkProductsCatalogHydrate") => {
                    let query = body["query"].as_str().expect("catalog hydrate query");
                    assert!(query.contains("bulkNested0: metafields("));
                    assert!(query.contains("namespace: \"custom\""));
                    Response {
                        status: 200,
                        headers: BTreeMap::new(),
                        body: json!({
                            "data": {
                                "products": {
                                    "nodes": [{
                                        "id": product_id,
                                        "title": "Nested product",
                                        "bulkNested0": {
                                            "nodes": [{
                                                "id": "gid://shopify/Metafield/701",
                                                "namespace": "custom",
                                                "key": "first",
                                                "value": "one"
                                            }],
                                            "pageInfo": {
                                                "hasNextPage": true,
                                                "endCursor": "product-metafield-page-1"
                                            }
                                        }
                                    }],
                                    "pageInfo": {
                                        "hasNextPage": false,
                                        "endCursor": "product-page-1"
                                    }
                                }
                            }
                        }),
                    }
                }
                Some("BulkProductNestedCatalogHydrate") => {
                    assert_eq!(body["variables"]["id"], json!(product_id));
                    assert_eq!(
                        body["variables"]["after"],
                        json!("product-metafield-page-1")
                    );
                    let query = body["query"].as_str().expect("nested hydrate query");
                    assert!(query.contains("product(id: $id)"));
                    assert!(query.contains("bulkNested0: metafields("));
                    Response {
                        status: 200,
                        headers: BTreeMap::new(),
                        body: json!({
                            "data": {
                                "product": {
                                    "bulkNested0": {
                                        "nodes": [{
                                            "id": "gid://shopify/Metafield/702",
                                            "namespace": "custom",
                                            "key": "second",
                                            "value": "two"
                                        }],
                                        "pageInfo": {
                                            "hasNextPage": false,
                                            "endCursor": "product-metafield-page-2"
                                        }
                                    }
                                }
                            }
                        }),
                    }
                }
                operation => panic!("unexpected operation: {operation:?}"),
            }
        });

        let response = proxy.process_request(test_request(
            r#"
            mutation RunNestedHydratedProductBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  products {
                    edges {
                      node {
                        id
                        title
                        metafields(namespace: "custom") {
                          edges { node { id namespace key value } }
                        }
                      }
                    }
                  }
                }
                "#
            }),
        ));

        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let failed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            failed.body["data"]["bulkOperation"]["status"],
            json!("FAILED")
        );
        assert_eq!(
            proxy
                .process_request(bulk_artifact_request(&operation_id))
                .status,
            404
        );
        assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn products_bulk_query_refuses_to_publish_an_artifact_when_catalog_hydration_fails() {
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_upstream_transport(|request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body is JSON");
            assert_eq!(body["operationName"], json!("BulkProductsCatalogHydrate"));
            Response {
                status: 503,
                headers: BTreeMap::new(),
                body: json!({ "errors": [{ "message": "Shopify unavailable" }] }),
            }
        });

        let response = proxy.process_request(test_request(
            r#"
            mutation RunColdProductBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "query": "{ products { edges { node { id } } } }" }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["status"],
            json!("CREATED")
        );
        let failed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            failed.body["data"]["bulkOperation"]["status"],
            json!("FAILED")
        );
        assert_eq!(
            failed.body["data"]["bulkOperation"]["errorCode"],
            json!("INTERNAL_SERVER_ERROR")
        );
        assert_eq!(
            proxy
                .process_request(bulk_artifact_request(&operation_id))
                .status,
            404
        );
    }

    #[test]
    fn product_variants_bulk_query_jsonl_materializes_supported_nested_child_connections() {
        let product_id = "gid://shopify/Product/variant-children";
        let media_id = "gid://shopify/MediaImage/variant-child";
        let mut product = seed_product(product_id, "Variant children", "variant-children");
        product.media = vec![json!({
            "id": media_id,
            "__typename": "MediaImage",
            "alt": "Variant media alt",
            "mediaContentType": "IMAGE",
            "status": "READY"
        })];
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_base_products(vec![product])
        .with_upstream_transport(|request| {
            if request.body.contains("OwnerMetafieldsHydrateNodes") {
                return Response {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: json!({ "data": { "nodes": [] } }),
                };
            }
            if request.body.contains("BulkProductVariantsCatalogHydrate") {
                return Response {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: json!({
                        "data": {
                            "productVariants": {
                                "nodes": [],
                                "pageInfo": {
                                    "hasNextPage": false,
                                    "endCursor": null
                                }
                            }
                        }
                    }),
                };
            }
            panic!("variant child bulk test should stay local")
        });
        let variant = create_variant(&mut proxy, product_id, "VARIANT-CHILD-SKU");
        let variant_id = variant["id"].as_str().unwrap().to_string();

        let append_media = proxy.process_request(test_request(
            r#"
            mutation AppendVariantMedia($productId: ID!, $variantMedia: [ProductVariantAppendMediaInput!]!) {
              productVariantAppendMedia(productId: $productId, variantMedia: $variantMedia) {
                productVariants { id }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "productId": product_id,
                "variantMedia": [{
                    "variantId": variant_id,
                    "mediaIds": [media_id]
                }]
            }),
        ));
        assert_eq!(append_media.status, 200);
        assert_eq!(
            append_media.body["data"]["productVariantAppendMedia"]["userErrors"],
            json!([])
        );

        let metafields = proxy.process_request(test_request(
            r#"
            mutation StageVariantChildMetafield($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) {
                metafields { id namespace key value }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "metafields": [{
                    "ownerId": variant_id,
                    "namespace": "custom",
                    "key": "care",
                    "type": "single_line_text_field",
                    "value": "wash cold"
                }]
            }),
        ));
        assert_eq!(metafields.status, 200);
        assert_eq!(
            metafields.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );
        let metafield_id = metafields.body["data"]["metafieldsSet"]["metafields"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let response = proxy.process_request(test_request(
            r#"
            mutation RunNestedVariantBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status objectCount rootObjectCount }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  productVariants(query: "sku:VARIANT-CHILD-SKU") {
                    edges {
                      node {
                        id
                        sku
                        media {
                          edges { node { id alt } }
                        }
                        metafields(first: 5, namespace: "custom") {
                          edges { node { id namespace key value } }
                        }
                      }
                    }
                  }
                }
                "#
            }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let completed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            completed.body["data"]["bulkOperation"]["status"],
            json!("COMPLETED")
        );
        let artifact = proxy.process_request(bulk_artifact_request(&operation_id));
        assert_eq!(artifact.status, 200);
        let rows = artifact
            .body
            .as_str()
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|row| {
            row == &json!({
                "id": variant_id,
                "sku": "VARIANT-CHILD-SKU"
            })
        }));
        assert!(
            rows.iter().any(|row| {
                row == &json!({
                    "id": media_id,
                    "alt": "Variant media alt",
                    "__parentId": variant_id
                })
            }),
            "nested variant rows: {rows:#?}"
        );
        assert!(rows.iter().any(|row| {
            row == &json!({
                "id": metafield_id,
                "namespace": "custom",
                "key": "care",
                "value": "wash cold",
                "__parentId": variant_id
            })
        }));

        let current = proxy.process_request(test_request(
            r#"
            query CurrentNestedVariantBulkQuery {
              currentBulkOperation(type: QUERY) {
                objectCount
                rootObjectCount
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["objectCount"],
            json!("3")
        );
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["rootObjectCount"],
            json!("1")
        );
    }

    #[test]
    fn product_variants_bulk_query_rejects_unsupported_nested_child_connections() {
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_base_products(vec![
            seed_product("gid://shopify/Product/1", "Red product", "red-product"),
            seed_product("gid://shopify/Product/2", "Blue product", "blue-product"),
        ])
        .with_upstream_transport(|request| {
            if request.body.contains("currentBulkOperation") {
                return Response {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: json!({ "data": { "currentBulkOperation": null } }),
                };
            }
            panic!("unsupported variant child bulk test should not call upstream")
        });
        create_variant(
            &mut proxy,
            "gid://shopify/Product/1",
            "UNSUPPORTED-VARIANT-CHILD-SKU",
        );

        let response = proxy.process_request(test_request(
            r#"
            mutation RunUnsupportedVariantChildBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  productVariants(query: "sku:UNSUPPORTED-VARIANT-CHILD-SKU") {
                    edges {
                      node {
                        id
                        sellingPlanGroups {
                          edges { node { id } }
                        }
                      }
                    }
                  }
                }
                "#
            }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["bulkOperation"],
            Value::Null
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([{
                "field": ["query"],
                "message": "Unsupported nested product variant connection in local bulk query: sellingPlanGroups. Supported nested product variant connections: media, metafields.",
                "code": "INVALID"
            }])
        );

        let current = proxy.process_request(test_request(
            r#"
            query CurrentBulkQueryAfterUnsupportedVariantChild {
              currentBulkOperation(type: QUERY) { id }
            }
            "#,
            json!({}),
        ));
        assert_eq!(current.body["data"]["currentBulkOperation"], Value::Null);
    }

    #[test]
    fn products_bulk_query_jsonl_materializes_supported_nested_child_connections() {
        let product_id = "gid://shopify/Product/nested-children";
        let media_id = "gid://shopify/MediaImage/nested-child";
        let collection_id = "gid://shopify/Collection/nested-child";
        let mut product = seed_product(product_id, "Nested children", "nested-children");
        product.media = vec![json!({
            "id": media_id,
            "__typename": "MediaImage",
            "alt": "Nested media alt",
            "mediaContentType": "IMAGE",
            "status": "READY"
        })];
        product.collections = vec![json!({
            "id": collection_id,
            "title": "Nested collection",
            "handle": "nested-collection"
        })];
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_base_products(vec![product]);

        let metafields = proxy.process_request(test_request(
            r#"
            mutation StageNestedChildMetafield($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) {
                metafields { id namespace key value }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "metafields": [{
                    "ownerId": product_id,
                    "namespace": "custom",
                    "key": "material",
                    "type": "single_line_text_field",
                    "value": "cotton"
                }]
            }),
        ));
        assert_eq!(metafields.status, 200);
        assert_eq!(
            metafields.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );
        let metafield_id = metafields.body["data"]["metafieldsSet"]["metafields"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let response = proxy.process_request(test_request(
            r#"
            mutation RunNestedProductBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status objectCount rootObjectCount }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  products {
                    edges {
                      node {
                        id
                        title
                        media {
                          edges { node { id alt } }
                        }
                        metafields {
                          edges { node { id namespace key value } }
                        }
                        collections {
                          edges { node { id title } }
                        }
                      }
                    }
                  }
                }
                "#
            }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunQuery"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let completed = read_bulk_operation(&mut proxy, &operation_id);
        assert_eq!(
            completed.body["data"]["bulkOperation"]["status"],
            json!("COMPLETED")
        );
        let artifact = proxy.process_request(bulk_artifact_request(&operation_id));
        assert_eq!(artifact.status, 200);
        let rows = artifact
            .body
            .as_str()
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(rows.len(), 4);
        assert!(rows.iter().any(|row| {
            row == &json!({
                "id": product_id,
                "title": "Nested children"
            })
        }));
        assert!(
            rows.iter().any(|row| {
                row == &json!({
                    "id": media_id,
                    "alt": "Nested media alt",
                    "__parentId": product_id
                })
            }),
            "nested product rows: {rows:#?}"
        );
        assert!(rows.iter().any(|row| {
            row == &json!({
                "id": metafield_id,
                "namespace": "custom",
                "key": "material",
                "value": "cotton",
                "__parentId": product_id
            })
        }));
        assert!(rows.iter().any(|row| {
            row == &json!({
                "id": collection_id,
                "title": "Nested collection",
                "__parentId": product_id
            })
        }));

        let current = proxy.process_request(test_request(
            r#"
            query CurrentNestedProductBulkQuery {
              currentBulkOperation(type: QUERY) {
                objectCount
                rootObjectCount
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["objectCount"],
            json!("4")
        );
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["rootObjectCount"],
            json!("1")
        );
    }

    #[test]
    fn products_bulk_query_rejects_unsupported_nested_child_connections() {
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
        })
        .with_base_products(vec![seed_product(
            "gid://shopify/Product/unsupported-child",
            "Unsupported child",
            "unsupported-child",
        )]);

        let response = proxy.process_request(test_request(
            r#"
            mutation RunUnsupportedProductChildBulkQuery($query: String!) {
              bulkOperationRunQuery(query: $query) {
                bulkOperation { id status }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "query": r#"
                {
                  products {
                    edges {
                      node {
                        id
                        sellingPlanGroups {
                          edges { node { id } }
                        }
                      }
                    }
                  }
                }
                "#
            }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["bulkOperation"],
            Value::Null
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunQuery"]["userErrors"],
            json!([{
                "field": ["query"],
                "message": "Unsupported nested product connection in local bulk query: sellingPlanGroups. Supported nested product connections: collections, images, media, metafields, variants.",
                "code": "INVALID"
            }])
        );

        let current = proxy.process_request(test_request(
            r#"
            query CurrentBulkQueryAfterUnsupportedChild {
              currentBulkOperation(type: QUERY) { id }
            }
            "#,
            json!({}),
        ));
        assert_eq!(current.body["data"]["currentBulkOperation"], Value::Null);
    }

    #[test]
    fn bulk_operation_run_mutation_applies_uploaded_product_updates_in_order() {
        let mut proxy = test_proxy();
        let jsonl = [
            json!({"product": {"id": "gid://shopify/Product/1", "title": "First bulk update"}})
                .to_string(),
            json!({"product": {"id": "gid://shopify/Product/1", "title": "First final bulk update"}})
                .to_string(),
            json!({"product": {"id": "gid://shopify/Product/2", "title": "Second bulk update"}})
                .to_string(),
            json!({"product": {"id": "gid://shopify/Product/2", "title": ""}}).to_string(),
        ]
        .join("\n")
            + "\n";
        let path =
            staged_bulk_mutation_upload_path_with_body(&mut proxy, "product-updates.jsonl", &jsonl);

        let response = proxy.process_request(test_request(
            r#"
            mutation RunBulkImport($mutation: String!, $path: String!) {
              bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
                bulkOperation { id status type objectCount rootObjectCount fileSize url }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "mutation": "mutation BulkProductUpdate($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }",
                "path": path
            }),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunMutation"]["userErrors"],
            json!([])
        );
        let operation_id = response.body["data"]["bulkOperationRunMutation"]["bulkOperation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            response.body["data"]["bulkOperationRunMutation"]["bulkOperation"]["status"],
            json!("CREATED")
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunMutation"]["bulkOperation"]["objectCount"],
            json!("0")
        );

        let current = proxy.process_request(test_request(
            r#"
            query CurrentBulkMutation {
              currentBulkOperation(type: MUTATION) {
                id
                status
                objectCount
                rootObjectCount
                fileSize
                url
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["id"],
            json!(operation_id)
        );
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["status"],
            json!("COMPLETED")
        );
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["objectCount"],
            json!("4")
        );
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["rootObjectCount"],
            json!("4")
        );

        let artifact = proxy.process_request(bulk_artifact_request(&operation_id));
        assert_eq!(artifact.status, 200);
        let artifact_body = artifact.body.as_str().unwrap();

        let read = proxy.process_request(test_request(
            r#"
            query ReadBulkUpdatedProducts($first: ID!, $second: ID!) {
              first: product(id: $first) { id title }
              second: product(id: $second) { id title }
            }
            "#,
            json!({
                "first": "gid://shopify/Product/1",
                "second": "gid://shopify/Product/2"
            }),
        ));
        assert_eq!(
            read.body["data"]["first"]["title"],
            json!("First final bulk update")
        );
        assert_eq!(
            read.body["data"]["second"]["title"],
            json!("Second bulk update")
        );

        assert_eq!(
            current.body["data"]["currentBulkOperation"]["fileSize"],
            json!(artifact_body.len().to_string())
        );
        let rows = artifact_body
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["__lineNumber"], json!(0));
        assert_eq!(
            rows[1]["data"]["productUpdate"]["product"]["title"],
            json!("First final bulk update")
        );
        assert_eq!(
            rows[2]["data"]["productUpdate"]["product"]["title"],
            json!("Second bulk update")
        );
        assert_eq!(rows[3]["__lineNumber"], json!(3));
        assert_eq!(
            rows[3]["data"]["productUpdate"]["userErrors"][0]["message"],
            json!("Title can't be blank")
        );
        assert_eq!(
            rows[3]["data"]["productUpdate"]["product"]["title"],
            json!("Second bulk update")
        );
    }

    #[test]
    fn bulk_operation_run_mutation_fails_unimplemented_inner_roots_without_upstream_writes() {
        let (proxy, upstream_calls) = instrumented_live_proxy();
        let commit_calls = Arc::new(Mutex::new(Vec::<Request>::new()));
        let commit_calls_for_transport = Arc::clone(&commit_calls);
        let mut proxy = proxy.with_commit_transport(move |request| {
            commit_calls_for_transport.lock().unwrap().push(request);
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({ "data": {} }),
            }
        });
        let jsonl = [
            json!({"urlRedirect": {"path": "/first", "target": "/first-target"}}).to_string(),
            json!({"urlRedirect": {"path": "/second", "target": "/second-target"}}).to_string(),
        ]
        .join("\n")
            + "\n";
        let path = staged_bulk_mutation_upload_path_with_body(
            &mut proxy,
            "unsupported-url-redirects.jsonl",
            &jsonl,
        );
        let log_len_before = proxy
            .process_request(meta_request("GET", "/__meta/log", ""))
            .body["entries"]
            .as_array()
            .unwrap()
            .len();

        let response = run_bulk_mutation_import(
            &mut proxy,
            "mutation UrlRedirectCreate($urlRedirect: UrlRedirectInput!) { urlRedirectCreate(urlRedirect: $urlRedirect) { urlRedirect { id path target } userErrors { field message } } }",
            &path,
        );

        assert_eq!(response.status, 200);
        assert!(
            upstream_calls.lock().unwrap().is_empty(),
            "valid-but-unimplemented bulk import rows must never write upstream during staging"
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunMutation"]["bulkOperation"]["status"],
            json!("CREATED")
        );
        assert_eq!(
            response.body["data"]["bulkOperationRunMutation"]["userErrors"],
            json!([])
        );

        let operation_id = response.body["data"]["bulkOperationRunMutation"]["bulkOperation"]["id"]
            .as_str()
            .unwrap();
        let current = proxy.process_request(test_request(
            r#"
            query CurrentBulkMutation {
              currentBulkOperation(type: MUTATION) {
                id status errorCode objectCount rootObjectCount fileSize url
              }
            }
            "#,
            json!({}),
        ));
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["status"],
            json!("FAILED")
        );
        assert_eq!(
            current.body["data"]["currentBulkOperation"]["objectCount"],
            json!("0")
        );

        let artifact = proxy.process_request(bulk_artifact_request(operation_id));
        assert_eq!(artifact.status, 200);
        let rows = artifact
            .body
            .as_str()
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["__lineNumber"], json!(0));
        assert_eq!(rows[1]["__lineNumber"], json!(1));
        assert!(rows.iter().all(|row| {
            row["errors"][0]["message"]
                == json!(
                    "Bulk mutation root `urlRedirectCreate` is accepted by Shopify but is not implemented locally. The proxy did not send this mutation upstream during draft staging."
                )
        }));

        let log = proxy.process_request(meta_request("GET", "/__meta/log", ""));
        assert_eq!(
            log.body["entries"].as_array().unwrap().len(),
            log_len_before,
            "unsupported import rows must not become commit-replay entries"
        );

        let commit = proxy.process_request(meta_request("POST", "/__meta/commit", ""));
        assert_eq!(commit.status, 200);
        assert_eq!(commit.body["committed"], json!(log_len_before));
        assert!(
            commit_calls.lock().unwrap().iter().all(|request| {
                !request.body.contains("urlRedirectCreate")
                    && !request.body.contains("bulkOperationRunMutation")
            }),
            "failed unsupported rows must not be replayed during commit"
        );
    }

    #[test]
    fn bulk_operation_run_mutation_rejects_invalid_inner_roots_without_upstream_writes() {
        let (mut proxy, upstream_calls) = instrumented_live_proxy();

        for mutation in [
            "mutation Broken(",
            "mutation NestedBulk($mutation: String!, $path: String!) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) { bulkOperation { id } } }",
        ] {
            let response = run_bulk_mutation_import(&mut proxy, mutation, "missing");

            assert_eq!(response.status, 200);
            assert_eq!(
                response.body["data"]["bulkOperationRunMutation"]["bulkOperation"],
                Value::Null
            );
            assert!(response.body["data"]["bulkOperationRunMutation"]["userErrors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty()));
        }

        assert!(
            upstream_calls.lock().unwrap().is_empty(),
            "invalid and disallowed inner roots must fail before upstream dispatch"
        );
    }

    #[test]
    fn bulk_operation_run_mutation_commit_replays_supported_rows_once_in_jsonl_order() {
        let (proxy, upstream_calls) = instrumented_live_proxy();
        let commit_calls = Arc::new(Mutex::new(Vec::<Request>::new()));
        let commit_calls_for_transport = Arc::clone(&commit_calls);
        let mut proxy = proxy
            .with_base_products(vec![seed_product(
                "gid://shopify/Product/1",
                "Original product",
                "original-product",
            )])
            .with_commit_transport(move |request| {
                commit_calls_for_transport.lock().unwrap().push(request);
                Response {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: json!({
                        "data": {
                            "productUpdate": {
                                "product": { "id": "gid://shopify/Product/1" },
                                "userErrors": []
                            }
                        }
                    }),
                }
            });
        let jsonl = [
            json!({"product": {"id": "gid://shopify/Product/1", "title": "First update"}})
                .to_string(),
            json!({"product": {"id": "gid://shopify/Product/1", "title": "Second update"}})
                .to_string(),
        ]
        .join("\n")
            + "\n";
        let path = staged_bulk_mutation_upload_path_with_body(
            &mut proxy,
            "ordered-product-updates.jsonl",
            &jsonl,
        );
        let log_len_before = proxy
            .process_request(meta_request("GET", "/__meta/log", ""))
            .body["entries"]
            .as_array()
            .unwrap()
            .len();
        let inner_mutation = "mutation ProductUpdate($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }";

        let response = run_bulk_mutation_import(&mut proxy, inner_mutation, &path);

        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["bulkOperationRunMutation"]["bulkOperation"]["status"],
            json!("CREATED")
        );
        assert!(
            upstream_calls.lock().unwrap().is_empty(),
            "locally implemented rows must remain local until commit"
        );

        let log = proxy.process_request(meta_request("GET", "/__meta/log", ""));
        let inner_entries = &log.body["entries"].as_array().unwrap()[log_len_before..];
        assert_eq!(inner_entries.len(), 2);
        assert_eq!(
            inner_entries[0]["variables"]["product"]["title"],
            json!("First update")
        );
        assert_eq!(
            inner_entries[1]["variables"]["product"]["title"],
            json!("Second update")
        );

        let first_commit = proxy.process_request(meta_request("POST", "/__meta/commit", ""));
        assert_eq!(first_commit.status, 200);
        let replayed_inner_bodies = commit_calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|request| {
                let body = serde_json::from_str::<Value>(&request.body).unwrap();
                (body["query"].as_str() == Some(inner_mutation)).then_some(body)
            })
            .collect::<Vec<_>>();
        assert_eq!(replayed_inner_bodies.len(), 2);
        assert_eq!(
            replayed_inner_bodies[0]["variables"]["product"]["title"],
            json!("First update")
        );
        assert_eq!(
            replayed_inner_bodies[1]["variables"]["product"]["title"],
            json!("Second update")
        );
        assert!(commit_calls
            .lock()
            .unwrap()
            .iter()
            .all(|request| { !request.body.contains("bulkOperationRunMutation") }));

        let second_commit = proxy.process_request(meta_request("POST", "/__meta/commit", ""));
        assert_eq!(second_commit.status, 200);
        assert_eq!(second_commit.body["committed"], json!(0));
        let replayed_inner_count = commit_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|request| {
                serde_json::from_str::<Value>(&request.body)
                    .ok()
                    .and_then(|body| body["query"].as_str().map(str::to_string))
                    .as_deref()
                    == Some(inner_mutation)
            })
            .count();
        assert_eq!(replayed_inner_count, 2);
    }

    #[test]
    fn bulk_operation_run_query_missing_query_returns_graphql_error() {
        let mut proxy = test_proxy();

        let response = proxy.process_request(test_request(
            r#"
            mutation MissingBulkQuery {
              bulkOperationRunQuery {
                bulkOperation { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({}),
        ));

        assert_eq!(response.status, 200);
        assert!(response.body.get("data").is_none());
        let errors = response.body["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0]["message"],
            json!("Field 'bulkOperationRunQuery' is missing required arguments: query")
        );
        assert_eq!(
            errors[0]["extensions"]["code"],
            json!("missingRequiredArguments")
        );
        assert_eq!(errors[0]["extensions"]["arguments"], json!("query"));
    }
}

/// Top-level root field name of a bulk query document (e.g. `products`, `orders`),
/// used to decide whether the proxy synthesizes JSONL locally or replays upstream.
fn bulk_query_root_field_name(query_text: &str) -> Option<String> {
    let document = parsed_document(query_text, &BTreeMap::new())?;
    document.root_fields.first().map(|field| field.name.clone())
}

#[derive(Default)]
struct BulkProductSearchHydrationRequirements {
    product_fields: BTreeSet<&'static str>,
    variant_fields: BTreeSet<&'static str>,
    needs_collections: bool,
}

struct BulkCatalogHydrationPlan {
    query: String,
    nested_connections: Vec<BulkCatalogNestedHydrationSpec>,
}

#[derive(Clone)]
struct BulkCatalogNestedHydrationSpec {
    name: String,
    response_key: String,
    arguments: BTreeMap<String, ResolvedValue>,
    node_selection: String,
}

impl BulkCatalogNestedHydrationSpec {
    fn selection(&self, first_variable: &str, after_variable: &str) -> String {
        format!(
            "{}: {}{} {{ nodes {{ {} }} pageInfo {{ hasNextPage endCursor }} }}",
            self.response_key,
            self.name,
            bulk_catalog_connection_arguments(&self.arguments, first_variable, after_variable,),
            self.node_selection,
        )
    }
}

fn bulk_product_search_hydration_requirements(
    query: Option<&ResolvedValue>,
) -> BulkProductSearchHydrationRequirements {
    let mut requirements = BulkProductSearchHydrationRequirements::default();
    let Some(ResolvedValue::String(query)) = query else {
        return requirements;
    };
    let Some(expression) = crate::proxy::search::parse_search_query(query) else {
        return requirements;
    };
    collect_bulk_product_search_hydration_requirements(&expression, &mut requirements);
    requirements
}

fn collect_bulk_product_search_hydration_requirements(
    expression: &crate::proxy::search::ParsedSearchExpression,
    requirements: &mut BulkProductSearchHydrationRequirements,
) {
    use crate::proxy::search::ParsedSearchExpression;
    match expression {
        ParsedSearchExpression::Term(term) => match term.field.as_deref() {
            Some("id") => {}
            Some("status") => {
                requirements.product_fields.insert("status");
            }
            Some("vendor") => {
                requirements.product_fields.insert("vendor");
            }
            Some("product_type") => {
                requirements.product_fields.insert("productType");
            }
            Some("title") => {
                requirements.product_fields.insert("title");
            }
            Some("handle") => {
                requirements.product_fields.insert("handle");
            }
            Some("tag" | "tag_not") => {
                requirements.product_fields.insert("tags");
            }
            Some("sku") => {
                requirements.variant_fields.insert("sku");
            }
            Some("barcode") => {
                requirements.variant_fields.insert("barcode");
            }
            Some("gift_card") => {
                requirements.product_fields.insert("isGiftCard");
            }
            Some("collection_id") => requirements.needs_collections = true,
            Some("published_status" | "published_at") => {
                requirements.product_fields.insert("publishedAt");
                requirements.product_fields.insert("status");
            }
            Some("created_at") => {
                requirements.product_fields.insert("createdAt");
            }
            Some("updated_at") => {
                requirements.product_fields.insert("updatedAt");
            }
            Some(_) => {}
            None => {
                requirements.product_fields.extend([
                    "title",
                    "handle",
                    "vendor",
                    "productType",
                    "tags",
                ]);
                requirements.variant_fields.insert("sku");
            }
        },
        ParsedSearchExpression::Not(expression) => {
            collect_bulk_product_search_hydration_requirements(expression, requirements);
        }
        ParsedSearchExpression::And(expressions) | ParsedSearchExpression::Or(expressions) => {
            for expression in expressions {
                collect_bulk_product_search_hydration_requirements(expression, requirements);
            }
        }
    }
}

fn bulk_catalog_hydration_plan(
    operation_name: &str,
    root_name: &str,
    field: &RootFieldSelection,
) -> BulkCatalogHydrationPlan {
    let requirements = bulk_product_search_hydration_requirements(field.arguments.get("query"));
    let node_selection = edge_node_selection(&field.selection);
    let (selection, nested_connections) =
        bulk_catalog_node_hydration_selection(root_name, &node_selection, requirements);
    let variables = if nested_connections.is_empty() {
        "$first: Int!, $after: String"
    } else {
        "$first: Int!, $after: String, $nestedFirst: Int!, $nestedAfter: String"
    };
    BulkCatalogHydrationPlan {
        query: format!(
            "query {operation_name}({variables}) {{ {root_name}(first: $first, after: $after) {{ nodes {{ {selection} }} pageInfo {{ hasNextPage endCursor }} }} }}"
        ),
        nested_connections,
    }
}

fn bulk_catalog_node_hydration_selection(
    root_name: &str,
    selected: &[SelectedField],
    requirements: BulkProductSearchHydrationRequirements,
) -> (String, Vec<BulkCatalogNestedHydrationSpec>) {
    let mut rendered = vec!["id".to_string()];
    let mut rendered_names = BTreeSet::from(["id".to_string()]);
    let mut nested_connections = Vec::new();
    let mut product_relation = None;
    let no_extra_children = BTreeSet::new();

    for field in selected {
        if field.name == "id" {
            continue;
        }
        if root_name == "productVariants" && field.name == "product" {
            product_relation = Some(field);
            continue;
        }
        if field_is_selected(&field.selection, "edges") {
            let extra_children = match field.name.as_str() {
                "variants" => &requirements.variant_fields,
                _ => &no_extra_children,
            };
            let connection =
                bulk_catalog_selected_connection(field, extra_children, nested_connections.len());
            rendered.push(connection.selection("$nestedFirst", "$nestedAfter"));
            nested_connections.push(connection);
        } else {
            rendered.push(crate::proxy::graphql_runtime::serialize_selected_field(
                &canonical_bulk_hydration_field(field),
            ));
        }
        rendered_names.insert(field.name.clone());
    }

    if root_name == "products" {
        for field in requirements.product_fields {
            if rendered_names.insert(field.to_string()) {
                rendered.push(field.to_string());
            }
        }
        if !requirements.variant_fields.is_empty() && rendered_names.insert("variants".to_string())
        {
            let connection = bulk_catalog_dependency_connection(
                "variants",
                &requirements.variant_fields,
                nested_connections.len(),
            );
            rendered.push(connection.selection("$nestedFirst", "$nestedAfter"));
            nested_connections.push(connection);
        }
        if requirements.needs_collections && rendered_names.insert("collections".to_string()) {
            let connection = bulk_catalog_dependency_connection(
                "collections",
                &BTreeSet::from(["id"]),
                nested_connections.len(),
            );
            rendered.push(connection.selection("$nestedFirst", "$nestedAfter"));
            nested_connections.push(connection);
        }
    } else {
        for field in &requirements.variant_fields {
            if rendered_names.insert((*field).to_string()) {
                rendered.push((*field).to_string());
            }
        }
        rendered.push(render_bulk_catalog_product_relation(
            product_relation,
            &requirements.product_fields,
        ));
    }

    (rendered.join(" "), nested_connections)
}

fn canonical_bulk_hydration_field(field: &SelectedField) -> SelectedField {
    let mut canonical = field.clone();
    canonical.response_key = canonical.name.clone();
    canonical.selection = canonical
        .selection
        .iter()
        .map(canonical_bulk_hydration_field)
        .collect();
    canonical
}

fn bulk_catalog_selected_connection(
    field: &SelectedField,
    extra_children: &BTreeSet<&'static str>,
    index: usize,
) -> BulkCatalogNestedHydrationSpec {
    let child_selection = edge_node_selection(&field.selection);
    let mut children = vec!["id".to_string()];
    let mut names = BTreeSet::from(["id".to_string()]);
    for child in &child_selection {
        if names.insert(child.name.clone()) {
            children.push(crate::proxy::graphql_runtime::serialize_selected_field(
                &canonical_bulk_hydration_field(child),
            ));
        }
    }
    for child in extra_children {
        if names.insert((*child).to_string()) {
            children.push((*child).to_string());
        }
    }
    BulkCatalogNestedHydrationSpec {
        name: field.name.clone(),
        response_key: format!("bulkNested{index}"),
        arguments: field.arguments.clone(),
        node_selection: children.join(" "),
    }
}

fn bulk_catalog_dependency_connection(
    name: &str,
    children: &BTreeSet<&'static str>,
    index: usize,
) -> BulkCatalogNestedHydrationSpec {
    let mut fields = vec!["id"];
    fields.extend(children.iter().copied().filter(|field| *field != "id"));
    BulkCatalogNestedHydrationSpec {
        name: name.to_string(),
        response_key: format!("bulkNested{index}"),
        arguments: BTreeMap::new(),
        node_selection: fields.join(" "),
    }
}

fn render_bulk_catalog_product_relation(
    selected: Option<&SelectedField>,
    extra_fields: &BTreeSet<&'static str>,
) -> String {
    let mut fields = vec!["id".to_string()];
    let mut names = BTreeSet::from(["id".to_string()]);
    if let Some(selected) = selected {
        for field in &selected.selection {
            if names.insert(field.name.clone()) {
                fields.push(crate::proxy::graphql_runtime::serialize_selected_field(
                    &canonical_bulk_hydration_field(field),
                ));
            }
        }
    }
    for field in extra_fields {
        if names.insert((*field).to_string()) {
            fields.push((*field).to_string());
        }
    }
    format!("product {{ {} }}", fields.join(" "))
}

fn bulk_catalog_connection_arguments(
    arguments: &BTreeMap<String, ResolvedValue>,
    first_variable: &str,
    after_variable: &str,
) -> String {
    let mut rendered = arguments
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "first" | "last" | "before" | "after"))
        .map(|(name, value)| {
            format!(
                "{name}: {}",
                crate::proxy::graphql_runtime::serialize_resolved_value(value)
            )
        })
        .collect::<Vec<_>>();
    rendered.push(format!("first: {first_variable}"));
    rendered.push(format!("after: {after_variable}"));
    format!("({})", rendered.join(", "))
}

fn merge_bulk_catalog_nodes(target: &mut Vec<Value>, incoming: &[Value]) {
    for incoming_node in incoming {
        let existing = incoming_node
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| {
                target
                    .iter_mut()
                    .find(|node| node.get("id").and_then(Value::as_str) == Some(id))
            });
        match (existing, incoming_node.as_object()) {
            (Some(existing), Some(incoming)) => {
                if let Some(existing) = existing.as_object_mut() {
                    existing.extend(incoming.clone());
                }
            }
            _ => target.push(incoming_node.clone()),
        }
    }
}

fn normalize_complete_bulk_catalog_nested_connections(
    node: &mut Value,
    nested_connections: &[BulkCatalogNestedHydrationSpec],
) -> bool {
    for connection in nested_connections {
        let Some(hydrated) = node.get(&connection.response_key).cloned() else {
            return false;
        };
        if hydrated.get("nodes").and_then(Value::as_array).is_none()
            || hydrated["pageInfo"]["hasNextPage"].as_bool() != Some(false)
        {
            return false;
        }
        let Some(node_object) = node.as_object_mut() else {
            return false;
        };
        node_object.remove(&connection.response_key);
        merge_bulk_catalog_connection(node_object, connection.name.as_str(), hydrated);
    }
    true
}

fn merge_bulk_catalog_connection(
    node: &mut serde_json::Map<String, Value>,
    name: &str,
    incoming: Value,
) {
    let Some(incoming_nodes) = incoming.get("nodes").and_then(Value::as_array) else {
        return;
    };
    if let Some(existing) = node.get_mut(name) {
        if let Some(existing_nodes) = existing.get_mut("nodes").and_then(Value::as_array_mut) {
            merge_bulk_catalog_nodes(existing_nodes, incoming_nodes);
            return;
        }
    }
    node.insert(name.to_string(), incoming);
}

fn bulk_operation_run_query_local_support_user_errors(query_text: &str) -> Option<Vec<Value>> {
    let document = parsed_document(query_text, &BTreeMap::new())?;
    let field = document.root_fields.first()?;
    let (object_label, supported_connections) = match field.name.as_str() {
        "products" => ("product", SUPPORTED_PRODUCT_BULK_CHILD_CONNECTIONS),
        "productVariants" => (
            "product variant",
            SUPPORTED_PRODUCT_VARIANT_BULK_CHILD_CONNECTIONS,
        ),
        _ => return None,
    };

    let node_selection = edge_node_selection(&field.selection);
    let unsupported =
        unsupported_local_bulk_nested_connection_paths(&node_selection, supported_connections);
    if unsupported.is_empty() {
        return None;
    }

    Some(vec![bulk_operation_run_query_user_error(&format!(
        "Unsupported nested {object_label} connection in local bulk query: {}. Supported nested {object_label} connections: {}.",
        unsupported.join(", "),
        supported_connections.join(", ")
    ))])
}

fn product_bulk_child_connection_supported(name: &str) -> bool {
    SUPPORTED_PRODUCT_BULK_CHILD_CONNECTIONS.contains(&name)
}

fn product_variant_bulk_child_connection_supported(name: &str) -> bool {
    SUPPORTED_PRODUCT_VARIANT_BULK_CHILD_CONNECTIONS.contains(&name)
}

fn unsupported_local_bulk_nested_connection_paths(
    selection: &[SelectedField],
    supported_direct_connections: &[&str],
) -> Vec<String> {
    let mut unsupported = Vec::new();
    let mut path = Vec::new();
    collect_unsupported_local_bulk_nested_connection_paths(
        selection,
        supported_direct_connections,
        &mut path,
        &mut unsupported,
    );
    unsupported
}

fn collect_unsupported_local_bulk_nested_connection_paths(
    selection: &[SelectedField],
    supported_direct_connections: &[&str],
    path: &mut Vec<String>,
    unsupported: &mut Vec<String>,
) {
    for field in selection {
        if field_is_selected(&field.selection, "edges") {
            let direct_connection = path.is_empty();
            if !direct_connection || !supported_direct_connections.contains(&field.name.as_str()) {
                let mut connection_path = path.clone();
                connection_path.push(field.name.clone());
                push_unique_string(unsupported, connection_path.join("."));
            }
            continue;
        }

        if field.selection.is_empty() {
            continue;
        }
        path.push(field.name.clone());
        collect_unsupported_local_bulk_nested_connection_paths(
            &field.selection,
            supported_direct_connections,
            path,
            unsupported,
        );
        path.pop();
    }
}

fn bulk_operation_result_artifact_path(id: &str) -> String {
    format!(
        "/__meta/bulk-operations/{}/result.jsonl",
        resource_id_path_tail(id)
    )
}

fn bulk_operation_result_artifact_url_for_port(port: u16, id: &str) -> String {
    format!(
        "https://localhost:{}{}",
        port,
        bulk_operation_result_artifact_path(id)
    )
}

fn bulk_jsonl_node_selection(selection: &[SelectedField]) -> Vec<SelectedField> {
    selection
        .iter()
        .filter(|field| !field_is_selected(&field.selection, "edges"))
        .cloned()
        .collect()
}

/// Bulk query text is GraphQL-as-data: Shopify's JSONL format contains exactly
/// the fields selected inside the `query:` argument, independently of the
/// outer API executor. Keep this syntax-aware projection local to bulk output;
/// ordinary Admin and Storefront responses are always projected by the schema
/// engine.
fn bulk_project_value(
    value: &Value,
    selection: &[SelectedField],
    api_version: crate::admin_graphql::AdminApiVersion,
) -> Value {
    if value.is_null() || selection.is_empty() {
        return value.clone();
    }
    if let Some(values) = value.as_array() {
        return Value::Array(
            values
                .iter()
                .map(|value| bulk_project_value(value, selection, api_version))
                .collect(),
        );
    }
    let Some(object) = value.as_object() else {
        return value.clone();
    };
    let record_type = value.get("__typename").and_then(Value::as_str).or_else(|| {
        value
            .get("id")
            .and_then(Value::as_str)
            .and_then(shopify_gid_resource_type)
    });
    let mut projected = serde_json::Map::new();
    for field in selection {
        if field.type_condition.as_deref().is_some_and(|condition| {
            record_type.is_some_and(|record_type| {
                condition != record_type
                    && !crate::admin_graphql::output_type_condition_applies(
                        api_version,
                        record_type,
                        condition,
                    )
            })
        }) {
            continue;
        }
        let Some(field_value) = object
            .get(&field.name)
            .or_else(|| object.get(&field.response_key))
        else {
            continue;
        };
        projected.insert(
            field.response_key.clone(),
            if field.selection.is_empty() || field_value.is_null() {
                field_value.clone()
            } else {
                bulk_project_value(field_value, &field.selection, api_version)
            },
        );
    }
    Value::Object(projected)
}

fn bulk_jsonl_child_node(mut node: Value, parent_id: &str) -> Value {
    if let Some(object) = node.as_object_mut() {
        object.insert("__parentId".to_string(), json!(parent_id));
    }
    node
}

fn values_to_jsonl(rows: Vec<Value>) -> String {
    let mut output = String::new();
    for row in rows {
        if let Ok(line) = serde_json::to_string(&row) {
            output.push_str(&line);
            output.push('\n');
        }
    }
    output
}

fn bulk_operation_result_metadata(jsonl: &str) -> (String, String) {
    (jsonl.lines().count().to_string(), jsonl.len().to_string())
}

fn bulk_operation_run_mutation_line_error(line_number: usize, message: &str) -> Value {
    json!({
        "errors": [{ "message": message }],
        "__lineNumber": line_number
    })
}

/// Mirrors the explicit Shopify-vs-proxy boundary for a root the schema-driven validator
/// accepts but the local JSONL synthesizer cannot emulate safely.
fn unsupported_bulk_query_root_error(root_name: &str) -> Value {
    user_error(
        ["query"],
        &format!(
            "Bulk query root `{root_name}` is accepted by Shopify's schema-driven validator but is not yet supported by the local JSONL synthesizer."
        ),
        None,
    )
}

fn bulk_operation_run_mutation_document_user_errors(
    mutation_text: &str,
    api_version: &str,
) -> Option<Vec<Value>> {
    let Some(document) = parsed_document(mutation_text, &BTreeMap::new()) else {
        return Some(vec![user_error(
            Value::Null,
            "Failed to parse the mutation - syntax error, unexpected end of file",
            Some("INVALID_MUTATION"),
        )]);
    };
    if document.operation_type != OperationType::Mutation {
        return Some(vec![user_error(
            Value::Null,
            "Invalid operation type. Only `mutation` operations are supported.",
            Some("INVALID_MUTATION"),
        )]);
    }
    if document.root_fields.len() != 1 {
        return Some(vec![user_error(
            ["mutation"],
            "You must specify a single top level mutation.",
            None,
        )]);
    }
    if matches!(
        document.root_fields[0].name.as_str(),
        "bulkOperationRunMutation" | "bulkOperationRunQuery"
    ) {
        return Some(vec![user_error(
            ["mutation"],
            "You must use an allowed mutation name.",
            None,
        )]);
    }
    let analysis = BulkMutationConnectionAnalysis::analyze(&document.root_fields, api_version);
    if analysis.connection_count > BULK_OPERATION_RUN_MUTATION_MAX_CONNECTIONS {
        return Some(vec![bulk_operation_run_mutation_user_error(
            "Bulk mutations cannot contain more than 1 connection.",
        )]);
    }
    if analysis.max_connection_depth > BULK_OPERATION_RUN_MUTATION_MAX_CONNECTION_DEPTH {
        return Some(vec![bulk_operation_run_mutation_user_error(
            "Bulk mutations cannot contain connections with a nesting depth greater than 1.",
        )]);
    }
    if let Some(user_error) = bulk_operation_query_storage_byte_limit_user_error(
        mutation_text,
        "is too large",
        Some("INVALID_MUTATION"),
    ) {
        return Some(vec![user_error]);
    }
    None
}

fn bulk_operation_run_mutation_user_error(message: &str) -> Value {
    user_error(["mutation"], message, None)
}

fn bulk_operation_run_mutation_client_identifier_user_errors(
    client_identifier: Option<&str>,
) -> Option<Vec<Value>> {
    let client_identifier = client_identifier?;
    let length = client_identifier.chars().count();
    if length < 10 {
        return Some(vec![user_error(
            ["clientIdentifier"],
            "is too short (minimum is 10 characters)",
            Some("INVALID_MUTATION"),
        )]);
    }
    if length > 255 {
        return Some(vec![user_error(
            ["clientIdentifier"],
            "is too long (maximum is 255 characters)",
            Some("INVALID_MUTATION"),
        )]);
    }
    None
}

fn bulk_operation_run_mutation_no_such_file_user_error() -> Value {
    user_error(
        Value::Null,
        "The JSONL file could not be found. Try uploading the file again, and check that you've entered the URL correctly for the stagedUploadPath mutation argument.",
        Some("NO_SUCH_FILE"),
    )
}

fn bulk_operation_run_mutation_file_size_too_large_user_error(max_file_size_bytes: u64) -> Value {
    let max_size_mb = max_file_size_bytes / (1024 * 1024);
    user_error(
        Value::Null,
        &format!("The input file size exceeds the maximum allowed size of {max_size_mb} MB."),
        Some("INVALID_STAGED_UPLOAD_FILE"),
    )
}

fn bulk_operation_run_mutation_empty_file_user_error() -> Value {
    user_error(
        Value::Null,
        "The input file is empty.",
        Some("INVALID_STAGED_UPLOAD_FILE"),
    )
}

fn bulk_operation_run_mutation_error_outcome(user_errors: Vec<Value>) -> ResolverOutcome<Value> {
    let payload = json!({
        "bulkOperation": null,
        "userErrors": user_errors
    });
    ResolverOutcome::value(payload)
}

#[derive(Default)]
struct BulkMutationConnectionAnalysis {
    connection_count: usize,
    max_connection_depth: usize,
}

impl BulkMutationConnectionAnalysis {
    fn analyze(fields: &[RootFieldSelection], api_version: &str) -> Self {
        let mut analysis = Self::default();
        for field in fields {
            analyze_bulk_mutation_field(
                api_version,
                "Mutation",
                &field.name,
                &field.selection,
                0,
                &mut analysis,
            );
        }
        analysis
    }
}

fn analyze_bulk_mutation_field(
    api_version: &str,
    parent_type: &str,
    field_name: &str,
    selection: &[SelectedField],
    connection_depth: usize,
    analysis: &mut BulkMutationConnectionAnalysis,
) {
    let Some(named_type) =
        crate::admin_graphql::output_field_named_type(api_version, parent_type, field_name)
    else {
        return;
    };
    let is_connection = named_type.ends_with("Connection");
    let next_connection_depth = connection_depth + usize::from(is_connection);
    if is_connection {
        analysis.connection_count += 1;
        analysis.max_connection_depth = analysis.max_connection_depth.max(next_connection_depth);
    }
    for child in selection {
        analyze_bulk_mutation_field(
            api_version,
            &named_type,
            &child.name,
            &child.selection,
            next_connection_depth,
            analysis,
        );
    }
}

#[derive(Default)]
struct BulkQueryAnalysis {
    connection_count: usize,
    max_connection_depth: usize,
    has_top_level_node: bool,
    has_connection_within_list: bool,
    nodes_connection_fields: Vec<String>,
    nested_without_parent_id_fields: Vec<String>,
}

impl BulkQueryAnalysis {
    fn analyze(fields: &[RootFieldSelection]) -> Self {
        let mut analysis = Self::default();
        for field in fields {
            if field.name == "node" {
                analysis.has_top_level_node = true;
            }
            analyze_bulk_query_field(
                &field.name,
                &field.selection,
                0,
                0,
                None,
                false,
                &mut analysis,
            );
        }
        analysis
    }
}

fn analyze_bulk_query_field(
    field_name: &str,
    selection: &[SelectedField],
    connection_depth: usize,
    list_depth: usize,
    parent_connection_name: Option<&str>,
    parent_node_has_unaliased_id: bool,
    analysis: &mut BulkQueryAnalysis,
) {
    if !field_is_selected(selection, "edges") {
        if field_is_selected(selection, "nodes") {
            push_unique_string(&mut analysis.nodes_connection_fields, field_name);
        }
        if let Some(nested_connection_name) = first_selected_connection_name(selection) {
            let next_list_depth = list_depth + usize::from(bulk_query_list_field(field_name));
            analyze_bulk_query_field(
                nested_connection_name,
                nested_connection_selection(selection, nested_connection_name),
                connection_depth,
                next_list_depth,
                parent_connection_name,
                parent_node_has_unaliased_id,
                analysis,
            );
        }
        return;
    }

    analysis.connection_count += 1;
    let depth = connection_depth + 1;
    analysis.max_connection_depth = analysis.max_connection_depth.max(depth);
    if list_depth > 0 {
        analysis.has_connection_within_list = true;
    }
    if let Some(parent_connection_name) = parent_connection_name {
        if !parent_node_has_unaliased_id {
            push_unique_string(
                &mut analysis.nested_without_parent_id_fields,
                parent_connection_name,
            );
        }
    }

    let node_selection = edge_node_selection(selection);
    let node_has_unaliased_id = node_selection.iter().any(|field| {
        field.name == "id" && field.response_key == "id" && field.selection.is_empty()
    });
    let next_list_depth = list_depth + usize::from(bulk_query_list_field(field_name));
    for child in &node_selection {
        analyze_bulk_query_field(
            &child.name,
            &child.selection,
            depth,
            next_list_depth,
            Some(field_name),
            node_has_unaliased_id,
            analysis,
        );
    }
}

fn first_selected_connection_name(selection: &[SelectedField]) -> Option<&str> {
    selection
        .iter()
        .find(|field| field_is_selected(&field.selection, "edges"))
        .map(|field| field.name.as_str())
}

fn nested_connection_selection<'a>(
    selection: &'a [SelectedField],
    connection_name: &str,
) -> &'a [SelectedField] {
    selection
        .iter()
        .find(|field| field.name == connection_name)
        .map(|field| field.selection.as_slice())
        .unwrap_or_default()
}

fn edge_node_selection(selection: &[SelectedField]) -> Vec<SelectedField> {
    selection
        .iter()
        .find(|field| field.name == "edges")
        .and_then(|edges| edges.selection.iter().find(|field| field.name == "node"))
        .map(|node| node.selection.clone())
        .unwrap_or_default()
}

fn field_is_selected(selection: &[SelectedField], name: &str) -> bool {
    selection.iter().any(|field| field.name == name)
}

fn bulk_query_list_field(name: &str) -> bool {
    matches!(name, "fulfillments")
}

fn bulk_operation_read_validation_errors(
    root_field: &str,
    response_key: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
    location: SourceLocation,
    operation_path: &str,
) -> Option<Vec<Value>> {
    match root_field {
        "bulkOperation" => {
            bulk_operation_id_validation_errors(response_key, arguments, location, operation_path)
        }
        "bulkOperations" => bulk_operations_argument_validation_errors(
            response_key,
            arguments,
            location,
            operation_path,
        ),
        _ => None,
    }
}

fn bulk_operation_id_validation_errors(
    response_key: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
    location: SourceLocation,
    operation_path: &str,
) -> Option<Vec<Value>> {
    let id = resolved_string_field(arguments, "id").unwrap_or_default();
    match shopify_gid_resource_type(&id) {
        Some("BulkOperation") => None,
        Some(_) => Some(vec![json!({
                "message": format!("Invalid id: {id}"),
                "locations": [{"line": location.line, "column": location.column}],
                "extensions": {"code": "RESOURCE_NOT_FOUND"},
                "path": [response_key]
        })]),
        None => Some(vec![json!({
                "message": format!("Invalid global id '{id}'"),
                "locations": [{"line": location.line, "column": location.column}],
                "path": [operation_path, response_key, "id"],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "CoercionError"
                }
        })]),
    }
}

fn bulk_operations_argument_validation_errors(
    response_key: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
    location: SourceLocation,
    operation_path: &str,
) -> Option<Vec<Value>> {
    if arguments.contains_key("first") && arguments.contains_key("last") {
        return Some(vec![json!({
                "message": "providing both first and last is not supported",
                "locations": [{"line": location.line, "column": location.column}],
                "extensions": {"code": "BAD_REQUEST"},
                "path": [response_key]
        })]);
    }
    if !arguments.contains_key("first") && !arguments.contains_key("last") {
        return Some(vec![json!({
                "message": "you must provide one of first or last",
                "locations": [{"line": location.line, "column": location.column}],
                "extensions": {"code": "BAD_REQUEST"},
                "path": [response_key]
        })]);
    }
    if matches!(
        resolved_string_field(arguments, "sortKey").as_deref(),
        Some("ID")
    ) {
        return Some(vec![json!({
                "message": "Argument 'sortKey' on Field 'bulkOperations' has an invalid value (ID). Expected type 'BulkOperationsSortKeys'.",
                "locations": [{"line": location.line, "column": location.column}],
                "path": [operation_path, response_key, "sortKey"],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": "sortKey"
                }
        })]);
    }
    if let Some(query) = resolved_string_field(arguments, "query") {
        if let Some(value) = bulk_operation_query_filter_value(&query, "created_at") {
            if !bulk_operation_valid_timestamp_filter(value) {
                return Some(vec![json!({
                        "message": "Invalid timestamp for query filter `created_at`.",
                        "locations": [{"line": location.line, "column": location.column}],
                        "extensions": {"code": "BAD_REQUEST"},
                        "path": [response_key]
                })]);
            }
        }
        if let Some(value) = bulk_operation_query_filter_value(&query, "id") {
            match shopify_gid_resource_type(value) {
                Some("BulkOperation") => {}
                Some(_) => {
                    return Some(vec![json!({
                            "message": format!("Invalid id: {value}"),
                            "locations": [{"line": location.line, "column": location.column}],
                            "extensions": {"code": "RESOURCE_NOT_FOUND"},
                            "path": [response_key]
                    })]);
                }
                None => {
                    return Some(vec![json!({
                            "message": format!("Invalid global id '{value}'"),
                            "locations": [{"line": location.line, "column": location.column}],
                            "path": [operation_path, response_key, "query"],
                            "extensions": {
                                "code": "argumentLiteralsIncompatible",
                                "typeName": "CoercionError"
                            }
                    })]);
                }
            }
        }
    }
    None
}

fn bulk_operation_sort_value(operation: &Value, sort_key: &str) -> String {
    let field = match sort_key {
        "COMPLETED_AT" => "completedAt",
        _ => "createdAt",
    };
    operation
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn bulk_operation_connection_sort_key(arguments: &BTreeMap<String, ResolvedValue>) -> String {
    resolved_string_field(arguments, "sortKey").unwrap_or_else(|| "CREATED_AT".to_string())
}

fn bulk_operation_connection_cursor(operation: &Value, sort_key: &str) -> String {
    bulk_operation_observed_cursor(operation, sort_key)
        .unwrap_or_else(|| bulk_operation_synthetic_cursor(operation, sort_key))
}

fn bulk_operation_observed_cursor(operation: &Value, sort_key: &str) -> Option<String> {
    operation
        .get(BULK_OPERATION_CURSORS_FIELD)
        .and_then(|cursors| cursors.get(sort_key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn bulk_operation_synthetic_cursor(operation: &Value, sort_key: &str) -> String {
    let id = operation
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let tail = resource_id_tail(id);
    let last_id = tail
        .parse::<u64>()
        .map(Value::from)
        .unwrap_or_else(|_| json!(tail));
    let mut cursor = serde_json::Map::new();
    cursor.insert("last_id".to_string(), last_id);

    let sort_value = bulk_operation_sort_value(operation, sort_key);
    if !sort_value.is_empty() {
        cursor.insert(
            "last_value".to_string(),
            json!(bulk_operation_cursor_timestamp(&sort_value)),
        );
    }

    base64::engine::general_purpose::STANDARD.encode(Value::Object(cursor).to_string())
}

fn bulk_operation_cursor_timestamp(value: &str) -> String {
    value.strip_suffix('Z').unwrap_or(value).replace('T', " ")
}

fn bulk_operation_concurrent_limit(request: &Request) -> usize {
    if admin_graphql_version(&request.path)
        .is_some_and(|version| version_at_least(version, 2026, 1))
    {
        5
    } else {
        1
    }
}

fn bulk_operation_is_non_terminal(operation: &Value) -> bool {
    !bulk_operation_status_is_terminal(operation.get("status").and_then(Value::as_str))
}

fn bulk_operation_status_is_terminal(status: Option<&str>) -> bool {
    matches!(
        status,
        Some("COMPLETED" | "FAILED" | "CANCELED" | "EXPIRED")
    )
}

fn bulk_operation_matches_query(
    operation: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let Some(query) = resolved_string_field(arguments, "query") else {
        return true;
    };
    for token in query.split_whitespace() {
        if token.eq_ignore_ascii_case("AND") {
            continue;
        }
        let Some((key, raw_value)) = token.split_once(':') else {
            continue;
        };
        let value = bulk_operation_clean_search_value(raw_value);
        let matches = match key.to_ascii_lowercase().as_str() {
            "id" => operation.get("id").and_then(Value::as_str) == Some(value),
            "operation_type" | "type" => {
                bulk_operation_valid_type_filter(value)
                    && operation
                        .get("type")
                        .and_then(Value::as_str)
                        .is_some_and(|operation_type| operation_type.eq_ignore_ascii_case(value))
            }
            "status" => {
                bulk_operation_valid_status_filter(value)
                    && operation
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|status| status.eq_ignore_ascii_case(value))
            }
            "created_at" => bulk_operation_matches_datetime_comparator(
                operation.get("createdAt").and_then(Value::as_str),
                value,
            ),
            _ => true,
        };
        if !matches {
            return false;
        }
    }
    true
}

fn bulk_operation_search_extension(
    response_key: &str,
    root_field: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if root_field != "bulkOperations" {
        return None;
    }
    let query = resolved_string_field(arguments, "query")?;
    let warning = bulk_operation_invalid_search_filter(&query)?;
    Some(json!([{
        "path": [response_key],
        "query": query,
        "parsed": {
            "field": warning.field,
            "match_all": warning.value
        },
        "warnings": [{
            "field": warning.field,
            "message": warning.message,
            "code": warning.code
        }]
    }]))
}

struct BulkOperationSearchWarning {
    field: String,
    value: String,
    message: String,
    code: &'static str,
}

fn bulk_operation_invalid_search_filter(query: &str) -> Option<BulkOperationSearchWarning> {
    for token in query.split_whitespace() {
        if token.eq_ignore_ascii_case("AND") {
            continue;
        }
        let Some((field, value)) = token.split_once(':') else {
            continue;
        };
        let value = bulk_operation_clean_search_value(value).to_string();
        match field.to_ascii_lowercase().as_str() {
            "status" => {
                if !bulk_operation_valid_status_filter(&value) {
                    return Some(bulk_operation_invalid_value_search_warning(field, value));
                }
            }
            "operation_type" | "type" => {
                if !bulk_operation_valid_type_filter(&value) {
                    return Some(bulk_operation_invalid_value_search_warning(field, value));
                }
            }
            "created_at" | "id" => {}
            _ => {
                return Some(BulkOperationSearchWarning {
                    field: field.to_string(),
                    value,
                    message: "Invalid search field for this query.".to_string(),
                    code: "invalid_field",
                });
            }
        }
    }
    None
}

fn bulk_operation_invalid_value_search_warning(
    field: &str,
    value: String,
) -> BulkOperationSearchWarning {
    BulkOperationSearchWarning {
        field: field.to_string(),
        message: format!("Input `{value}` is not an accepted value."),
        value,
        code: "invalid_value",
    }
}

fn bulk_operation_query_filter_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split_whitespace().find_map(|token| {
        let (candidate, value) = token.split_once(':')?;
        (candidate == key).then_some(bulk_operation_clean_search_value(value))
    })
}

fn bulk_operation_clean_search_value(value: &str) -> &str {
    value.trim_matches('"').trim_matches('\'')
}

fn bulk_operation_valid_timestamp_filter(value: &str) -> bool {
    let value = bulk_operation_clean_search_value(value);
    let (_, expected) = bulk_operation_search_comparator(value);
    bulk_operation_valid_date_or_datetime_filter(expected)
}

fn bulk_operation_valid_date_or_datetime_filter(value: &str) -> bool {
    if value.len() == "2026-05-05".len() {
        return value.chars().enumerate().all(|(index, character)| {
            matches!(index, 4 | 7)
                .then_some(character == '-')
                .unwrap_or_else(|| character.is_ascii_digit())
        });
    }
    value.len() >= "2026-05-05T20:32:29Z".len()
        && value.chars().nth(4) == Some('-')
        && value.chars().nth(7) == Some('-')
        && value.contains('T')
        && value.ends_with('Z')
}

fn bulk_operation_valid_status_filter(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "CREATED" | "RUNNING" | "COMPLETED" | "CANCELING" | "CANCELED" | "FAILED"
    )
}

fn bulk_operation_valid_type_filter(value: &str) -> bool {
    matches!(value.to_ascii_uppercase().as_str(), "QUERY" | "MUTATION")
}

fn bulk_operation_matches_datetime_comparator(actual: Option<&str>, query_value: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let query_value = bulk_operation_clean_search_value(query_value);
    if query_value.is_empty() {
        return false;
    }
    let (operator, expected) = bulk_operation_search_comparator(query_value);
    if expected.is_empty() {
        return false;
    }
    let actual = bulk_operation_datetime_value(actual, expected);
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual.starts_with(expected),
    }
}

fn bulk_operation_search_comparator(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("=", value)
}

fn bulk_operation_datetime_value<'a>(actual: &'a str, expected: &str) -> &'a str {
    if expected.contains('T') {
        actual
    } else {
        actual
            .split_once('T')
            .map(|(date, _)| date)
            .unwrap_or(actual)
    }
}

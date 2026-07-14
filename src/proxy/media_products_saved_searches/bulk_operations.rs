use super::owner_metafields::{
    metafield_cursor, owner_metafield_key_position, owner_metafield_with_connection_key,
    owner_metafields_connection_keys,
};
use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn dispatch_bulk_operations_graphql(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation: &crate::graphql::ParsedOperation,
        root_field: &str,
        execution: CapabilityExecution,
    ) -> Response {
        match execution {
            CapabilityExecution::OverlayRead
                if operation.operation_type == OperationType::Query =>
            {
                self.bulk_operation_read_response(request, query, variables, root_field)
            }
            CapabilityExecution::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "bulkOperationRunQuery" =>
            {
                self.bulk_operation_run_query(request, query, variables)
            }
            CapabilityExecution::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "bulkOperationRunMutation" =>
            {
                self.bulk_operation_run_mutation(request, query, variables)
            }
            CapabilityExecution::StageLocally
                if operation.operation_type == OperationType::Mutation
                    && root_field == "bulkOperationCancel" =>
            {
                self.bulk_operation_cancel(request, query, variables)
            }
            CapabilityExecution::OverlayRead | CapabilityExecution::StageLocally => {
                Self::dispatch_capability_fallback(execution, root_field)
            }
            CapabilityExecution::Passthrough => {
                unreachable!("non-unknown passthrough capabilities are not registered")
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

// Canonical mutation forwarded to upstream when a schema-valid bulk query root is
// accepted by the validator but is not one of the locally synthesized roots
// (`products`/`productVariants`). LiveHybrid replays the recorded upstream
// `bulkOperationRunQuery` response unchanged. This text must stay byte-identical to
// the cassette's recorded `query`, since the strict cassette matches query text exactly.
const BULK_OPERATION_RUN_QUERY_PROXY_FALLBACK_QUERY: &str = "mutation BulkOperationRunQueryProxyFallback($query: String!) { bulkOperationRunQuery(query: $query) { bulkOperation { id status type } userErrors { field message code } } }";

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

    fn bulk_operation_run_query_result(&self, query_text: &str) -> BulkOperationRunQueryResult {
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
            "products" => self.bulk_operation_products_result(field),
            "productVariants" => self.bulk_operation_product_variants_result(field),
            _ => BulkOperationRunQueryResult {
                jsonl: String::new(),
                root_object_count: 0,
            },
        }
    }

    fn bulk_operation_products_result(
        &self,
        field: &RootFieldSelection,
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
            let product_json = bulk_jsonl_projected_node(
                self.product_owner_json_with_store_currency(
                    &product,
                    &variants,
                    &product_selection,
                ),
                &product_selection,
            );
            rows.push(product_json);

            for selection in &nested_connections {
                for child in self.bulk_jsonl_product_child_rows(&product, &variants, selection) {
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
                .map(|collection| selected_json(collection, &child_node_selection))
                .collect(),
            "images" => product
                .media
                .iter()
                .filter_map(product_image_json_from_media)
                .map(|image| selected_json(&image, &child_node_selection))
                .collect(),
            "media" => product
                .media
                .iter()
                .map(|media| selected_json(media, &child_node_selection))
                .collect(),
            "metafields" => self
                .bulk_owner_metafield_nodes(
                    &product.id,
                    product.extra_fields.get("metafields"),
                    selection,
                )
                .into_iter()
                .map(|metafield| {
                    self.selected_reference_value_record_json(&metafield, &child_node_selection)
                })
                .collect(),
            "variants" => variants
                .iter()
                .map(|variant| {
                    self.product_variant_json_with_current_publication_context(
                        variant,
                        Some(product),
                        &child_node_selection,
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        rows.into_iter()
            .map(|row| bulk_jsonl_projected_node(row, &child_node_selection))
            .collect()
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
                rows.push(bulk_jsonl_projected_node(
                    self.product_variant_json_with_current_publication_context(
                        &variant,
                        Some(&product),
                        &variant_selection,
                    ),
                    &variant_selection,
                ));

                for selection in &nested_connections {
                    for child in
                        self.bulk_jsonl_product_variant_child_rows(&product, &variant, selection)
                    {
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
    ) -> Vec<Value> {
        let child_node_selection = edge_node_selection(&selection.selection);
        let child_node_selection = bulk_jsonl_node_selection(&child_node_selection);
        if child_node_selection.is_empty() {
            return Vec::new();
        }

        let rows = match selection.name.as_str() {
            "media" => variant_attached_media_nodes(variant, Some(product))
                .iter()
                .map(|media| selected_json(media, &child_node_selection))
                .collect(),
            "metafields" => self
                .bulk_owner_metafield_nodes(
                    &variant.id,
                    variant.extra_fields.get("metafields"),
                    selection,
                )
                .into_iter()
                .map(|metafield| {
                    self.selected_reference_value_record_json(&metafield, &child_node_selection)
                })
                .collect(),
            _ => Vec::new(),
        };
        rows.into_iter()
            .map(|row| bulk_jsonl_projected_node(row, &child_node_selection))
            .collect()
    }

    pub(in crate::proxy) fn bulk_operation_read_response(
        &self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
    ) -> Response {
        let Some(fields) = self.execution_root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        if self.should_passthrough_cold_bulk_operations_read(&fields) {
            return (self.upstream_transport)(request.clone());
        }
        let operation_path = parsed_document(query, variables)
            .map(|document| document.operation_path)
            .unwrap_or_else(|| "query".to_string());
        if let Some(response) =
            self.bulk_operation_read_validation_response(&fields, root_field, &operation_path)
        {
            return response;
        }
        let data = self.bulk_operation_read_data(&fields);
        let mut body = json!({ "data": data });
        if let Some(search) = bulk_operation_search_extensions(&fields) {
            body["extensions"] = json!({ "search": search });
        }
        ok_json(body)
    }

    fn should_passthrough_cold_bulk_operations_read(&self, fields: &[RootFieldSelection]) -> bool {
        self.config.read_mode == ReadMode::LiveHybrid
            && self.store.staged.bulk_operations.is_empty()
            && fields.iter().all(|field| {
                field.name == "bulkOperations"
                    && field.arguments.contains_key("sortKey")
                    && !field.arguments.contains_key("query")
            })
    }

    pub(in crate::proxy) fn bulk_operation_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "bulkOperation" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.bulk_operation_by_id(&id)
                        .map(|operation| selected_json(operation, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "bulkOperations" => self.bulk_operations_connection(field),
                "currentBulkOperation" => {
                    let operation_type = resolved_string_field(&field.arguments, "type")
                        .unwrap_or_else(|| "QUERY".to_string());
                    self.current_bulk_operation(&operation_type)
                        .map(|operation| selected_json(operation, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => return None,
            })
        })
    }

    fn bulk_operation_read_validation_response(
        &self,
        fields: &[RootFieldSelection],
        root_field: &str,
        operation_path: &str,
    ) -> Option<Response> {
        let field = fields.iter().find(|field| field.name == root_field)?;
        match field.name.as_str() {
            "bulkOperation" => bulk_operation_id_validation_response(field, operation_path),
            "bulkOperations" => bulk_operations_argument_validation_response(field, operation_path),
            _ => None,
        }
    }

    fn bulk_operation_by_id(&self, id: &str) -> Option<&Value> {
        self.store.staged.bulk_operations.get(id)
    }

    fn effective_bulk_operations(&self) -> Vec<&Value> {
        let mut operations = self
            .store
            .staged
            .bulk_operations
            .values()
            .collect::<Vec<_>>();
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

    fn current_bulk_operation(&self, operation_type: &str) -> Option<&Value> {
        self.effective_bulk_operations()
            .into_iter()
            .find(|operation| operation.get("type").and_then(Value::as_str) == Some(operation_type))
    }

    fn bulk_operations_connection(&self, field: &RootFieldSelection) -> Value {
        let mut operations = self.effective_bulk_operations();
        operations.retain(|operation| bulk_operation_matches_query(operation, &field.arguments));

        let sort_key = resolved_string_field(&field.arguments, "sortKey")
            .unwrap_or_else(|| "CREATED_AT".to_string());
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
        let records = operations.into_iter().cloned().collect::<Vec<_>>();
        selected_connection_json_with_args(
            records,
            &field.arguments,
            &field.selection,
            |operation| {
                operation
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
        )
    }

    pub(in crate::proxy) fn bulk_operation_run_query(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "bulkOperationRunQuery".to_string()
            });
        let query_text = resolved_string_field(&arguments, "query").unwrap_or_else(|| {
            "#graphql\n{ products { edges { node { id title } } } }".to_string()
        });
        if let Some(user_errors) = bulk_operation_run_query_user_errors(&query_text) {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": user_errors
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
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
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        // Shopify validates bulk queries against the Admin GraphQL schema, so the proxy
        // accepts schema-valid roots beyond the ones it can synthesize JSONL for locally.
        // Local synthesis is scoped to `products`/`productVariants`; every other accepted
        // root is replayed from the recorded upstream `bulkOperationRunQuery` response under
        // LiveHybrid (returning Shopify's real BulkOperation id) rather than minting a
        // synthetic operation we cannot faithfully export.
        let root_name = bulk_query_root_field_name(&query_text);
        let locally_synthesized = matches!(
            root_name.as_deref(),
            Some("products") | Some("productVariants")
        );
        if !locally_synthesized {
            if let Some(payload) =
                self.bulk_operation_run_query_upstream_payload(request, &query_text)
            {
                return ok_json(json!({ "data": { response_key: payload } }));
            }
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [unsupported_bulk_query_root_error(
                    root_name.as_deref().unwrap_or_default()
                )]
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }
        if let Some(user_errors) = bulk_operation_run_query_local_support_user_errors(&query_text) {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": user_errors
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        let id = self.next_bulk_operation_gid();
        let created_at = self.next_product_timestamp();
        let result = self.bulk_operation_run_query_result(&query_text);
        let (object_count, file_size) = bulk_operation_result_metadata(&result.jsonl);
        let root_object_count = result.root_object_count.to_string();
        let terminal_operation = self.bulk_operation_record(BulkOperationRecordSpec {
            id: &id,
            status: "COMPLETED",
            operation_type: "QUERY",
            query: &query_text,
            count: &object_count,
            root_count: &root_object_count,
            created_at: &created_at,
            file_size: &file_size,
        });
        self.stage_bulk_operation_result(&id, result.jsonl);
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), terminal_operation);
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "bulkOperationRunQuery",
            vec![id.clone()],
        );

        let payload = json!({
            "bulkOperation": self.bulk_operation_record(BulkOperationRecordSpec {
                id: &id,
                status: "CREATED",
                operation_type: "QUERY",
                query: &query_text,
                count: "0",
                root_count: "0",
                created_at: &created_at,
                file_size: "0",
            }),
            "userErrors": []
        });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    /// Forwards the canonical `BulkOperationRunQueryProxyFallback` mutation upstream for a
    /// schema-valid bulk query root the proxy does not synthesize locally, returning the
    /// recorded `bulkOperationRunQuery` payload unchanged. Returns `None` when not in
    /// LiveHybrid or when the upstream response does not carry a payload object.
    fn bulk_operation_run_query_upstream_payload(
        &self,
        request: &Request,
        query_text: &str,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": BULK_OPERATION_RUN_QUERY_PROXY_FALLBACK_QUERY,
                "operationName": "BulkOperationRunQueryProxyFallback",
                "variables": { "query": query_text }
            }),
        );
        if response.status >= 400 {
            return None;
        }
        response
            .body
            .get("data")
            .and_then(|data| data.get("bulkOperationRunQuery"))
            .filter(|payload| payload.is_object())
            .cloned()
    }

    pub(in crate::proxy) fn bulk_operation_run_mutation(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let (response_key, payload_selection, arguments) = self
            .execution_primary_root_response_parts(query, variables, || {
                "bulkOperationRunMutation".to_string()
            });
        let mutation_text = resolved_string_field(&arguments, "mutation").unwrap_or_default();
        let staged_upload_path =
            resolved_string_field(&arguments, "stagedUploadPath").unwrap_or_default();
        let client_identifier = resolved_string_field(&arguments, "clientIdentifier");

        let api_version = admin_graphql_version(&request.path)
            .unwrap_or_else(|| latest_supported_admin_graphql_version().unwrap_or("2026-04"));
        if let Some(user_errors) =
            bulk_operation_run_mutation_document_user_errors(&mutation_text, api_version)
        {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                user_errors,
            );
        }
        if let Some(user_errors) =
            bulk_operation_run_mutation_client_identifier_user_errors(client_identifier.as_deref())
        {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                user_errors,
            );
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
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                vec![bulk_operation_run_mutation_file_size_too_large_user_error(
                    max_file_size,
                )],
            );
        }
        if staged_upload_file_size.flatten() == Some(0) {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                vec![bulk_operation_run_mutation_empty_file_user_error()],
            );
        }
        if staged_upload_file_size.is_none() {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                vec![bulk_operation_run_mutation_no_such_file_user_error()],
            );
        }
        let Some(staged_upload_body) = self
            .bulk_operation_staged_upload_body(&staged_upload_path)
            .map(str::to_string)
        else {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                vec![bulk_operation_run_mutation_no_such_file_user_error()],
            );
        };
        if let Some(operation_id) = self.throttled_bulk_operation_id("MUTATION", request) {
            return bulk_operation_run_mutation_error_response(
                &response_key,
                &payload_selection,
                vec![user_error(
                    Value::Null,
                    &format!("A bulk mutation operation for this app and shop is already in progress: {operation_id}."),
                    Some("OPERATION_IN_PROGRESS"),
                )],
            );
        }

        let id = self.next_bulk_operation_gid();
        let created_at = self.next_product_timestamp();
        let result_jsonl = self.bulk_operation_run_mutation_result_jsonl(
            request,
            &mutation_text,
            &staged_upload_body,
        );
        let (object_count, file_size) = bulk_operation_result_metadata(&result_jsonl);
        let terminal_operation = self.bulk_operation_record(BulkOperationRecordSpec {
            id: &id,
            status: "COMPLETED",
            operation_type: "MUTATION",
            query: &mutation_text,
            count: &object_count,
            root_count: &object_count,
            created_at: &created_at,
            file_size: &file_size,
        });
        self.stage_bulk_operation_result(&id, result_jsonl);
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
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
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

    fn bulk_operation_run_mutation_result_jsonl(
        &mut self,
        request: &Request,
        mutation_text: &str,
        jsonl: &str,
    ) -> String {
        let mut rows = Vec::new();
        for (line_number, line) in jsonl.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let variables = match serde_json::from_str::<Value>(line) {
                Ok(Value::Object(variables)) => Value::Object(variables),
                Ok(other) => {
                    rows.push(bulk_operation_run_mutation_line_error(
                        line_number,
                        &format!("Expected JSON object variables, got {other}"),
                    ));
                    continue;
                }
                Err(error) => {
                    rows.push(bulk_operation_run_mutation_line_error(
                        line_number,
                        &format!("Failed to parse JSONL variables: {error}"),
                    ));
                    continue;
                }
            };
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
            let mut row = self.resolve_nested_graphql_request(&row_request).body;
            if let Some(object) = row.as_object_mut() {
                object.insert("__lineNumber".to_string(), json!(line_number));
            } else {
                row = json!({
                    "data": row,
                    "__lineNumber": line_number
                });
            }
            rows.push(row);
        }
        values_to_jsonl(rows)
    }

    pub(in crate::proxy) fn bulk_operation_cancel(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let id = resolved_string_field(variables, "id").unwrap_or_default();
        let (response_key, payload_selection) =
            self.execution_primary_root_response_selection(query, variables, || {
                "bulkOperationCancel".to_string()
            });

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
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
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
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }

        let mut operation = existing_operation;
        operation["status"] = json!("CANCELING");
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), operation.clone());
        self.record_mutation_log_entry(request, query, variables, "bulkOperationCancel", vec![id]);
        let payload = json!({ "bulkOperation": operation, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
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
                .staged
                .bulk_operations
                .insert(id.to_string(), operation);
        }
    }
}

fn bulk_operation_record_value(spec: BulkOperationRecordSpec<'_>, artifact_url: String) -> Value {
    let completed = spec.status == "COMPLETED";
    let file_size_value = if completed {
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
        "completedAt": if completed { json!(spec.created_at) } else { Value::Null },
        "objectCount": if completed { spec.count } else { "0" },
        "rootObjectCount": if completed { spec.root_count } else { "0" },
        "fileSize": file_size_value,
        "url": if completed { json!(artifact_url) } else { Value::Null },
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
        .with_upstream_transport(|_| panic!("bulk operation tests should not call upstream"))
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
        let mut proxy = test_proxy();
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

fn bulk_jsonl_projected_node(mut node: Value, selection: &[SelectedField]) -> Value {
    let selects_unaliased_typename = selection
        .iter()
        .any(|field| field.name == "__typename" && field.response_key == "__typename");
    if !selects_unaliased_typename {
        if let Some(object) = node.as_object_mut() {
            object.remove("__typename");
        }
    }
    node
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

/// Mirrors Shopify-vs-proxy divergence: a root the schema-driven validator accepts but
/// the local JSONL synthesizer cannot emulate, surfaced only when no upstream replay is
/// available (e.g. outside LiveHybrid).
fn unsupported_bulk_query_root_error(root_name: &str) -> Value {
    user_error(
        ["query"],
        &format!(
            "Bulk query root `{root_name}` is accepted by Shopify's schema-driven validator but is not yet supported by the local JSONL synthesizer."
        ),
        Some("UNSUPPORTED_IN_PROXY"),
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

fn bulk_operation_run_mutation_error_response(
    response_key: &str,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Response {
    let payload = json!({
        "bulkOperation": null,
        "userErrors": user_errors
    });
    ok_json(json!({ "data": { response_key: selected_json(&payload, payload_selection) } }))
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
    selected_child_selection(selection, "edges")
        .and_then(|edge_selection| selected_child_selection(&edge_selection, "node"))
        .unwrap_or_default()
}

fn field_is_selected(selection: &[SelectedField], name: &str) -> bool {
    selection.iter().any(|field| field.name == name)
}

fn bulk_query_list_field(name: &str) -> bool {
    matches!(name, "fulfillments")
}

fn bulk_operation_id_validation_response(
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Response> {
    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
    match shopify_gid_resource_type(&id) {
        Some("BulkOperation") => None,
        Some(_) => Some(ok_json(json!({
            "errors": [{
                "message": format!("Invalid id: {id}"),
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "RESOURCE_NOT_FOUND"},
                "path": [field.response_key]
            }],
            "data": { field.response_key.clone(): null }
        }))),
        None => Some(ok_json(json!({
            "errors": [{
                "message": format!("Invalid global id '{id}'"),
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "path": [operation_path, field.response_key.clone(), "id"],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "CoercionError"
                }
            }]
        }))),
    }
}

fn bulk_operations_argument_validation_response(
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Response> {
    if field.arguments.contains_key("first") && field.arguments.contains_key("last") {
        return Some(ok_json(json!({
            "errors": [{
                "message": "providing both first and last is not supported",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "BAD_REQUEST"},
                "path": [field.response_key]
            }],
            "data": null
        })));
    }
    if !field.arguments.contains_key("first") && !field.arguments.contains_key("last") {
        return Some(ok_json(json!({
            "errors": [{
                "message": "you must provide one of first or last",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "BAD_REQUEST"},
                "path": [field.response_key]
            }],
            "data": null
        })));
    }
    if matches!(
        resolved_string_field(&field.arguments, "sortKey").as_deref(),
        Some("ID")
    ) {
        return Some(ok_json(json!({
            "errors": [{
                "message": "Argument 'sortKey' on Field 'bulkOperations' has an invalid value (ID). Expected type 'BulkOperationsSortKeys'.",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "path": [operation_path, field.response_key.clone(), "sortKey"],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": "sortKey"
                }
            }]
        })));
    }
    if let Some(query) = resolved_string_field(&field.arguments, "query") {
        if let Some(value) = bulk_operation_query_filter_value(&query, "created_at") {
            if !bulk_operation_valid_timestamp_filter(value) {
                return Some(ok_json(json!({
                    "errors": [{
                        "message": "Invalid timestamp for query filter `created_at`.",
                        "locations": [{"line": field.location.line, "column": field.location.column}],
                        "extensions": {"code": "BAD_REQUEST"},
                        "path": [field.response_key]
                    }],
                    "data": null
                })));
            }
        }
        if let Some(value) = bulk_operation_query_filter_value(&query, "id") {
            match shopify_gid_resource_type(value) {
                Some("BulkOperation") => {}
                Some(_) => {
                    return Some(ok_json(json!({
                        "errors": [{
                            "message": format!("Invalid id: {value}"),
                            "locations": [{"line": field.location.line, "column": field.location.column}],
                            "extensions": {"code": "RESOURCE_NOT_FOUND"},
                            "path": [field.response_key]
                        }],
                        "data": { field.response_key.clone(): null }
                    })));
                }
                None => {
                    return Some(ok_json(json!({
                        "errors": [{
                            "message": format!("Invalid global id '{value}'"),
                            "locations": [{"line": field.location.line, "column": field.location.column}],
                            "path": [operation_path, field.response_key.clone(), "query"],
                            "extensions": {
                                "code": "argumentLiteralsIncompatible",
                                "typeName": "CoercionError"
                            }
                        }]
                    })));
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

fn bulk_operation_search_extensions(fields: &[RootFieldSelection]) -> Option<Value> {
    let warnings = fields
        .iter()
        .filter(|field| field.name == "bulkOperations")
        .filter_map(|field| {
            let query = resolved_string_field(&field.arguments, "query")?;
            let warning = bulk_operation_invalid_search_filter(&query)?;
            Some(json!({
                "path": [field.response_key.clone()],
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
            }))
        })
        .collect::<Vec<_>>();
    (!warnings.is_empty()).then_some(Value::Array(warnings))
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

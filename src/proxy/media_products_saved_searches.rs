use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn bulk_operation_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "bulkOperation" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .bulk_operations
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

    pub(in crate::proxy) fn bulk_operation_run_query(
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
        if let Some(user_errors) = bulk_operation_run_query_user_errors(&query_text) {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": user_errors
            });
            return ok_json(
                json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }),
            );
        }
        if let Some(operation_id) = self.in_progress_query_bulk_operation_id() {
            let payload = json!({
                "bulkOperation": null,
                "userErrors": [{
                    "field": null,
                    "message": format!("A bulk query operation for this app and shop is already in progress: {operation_id}."),
                    "code": "OPERATION_IN_PROGRESS"
                }]
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
            "bulkOperation": bulk_operation_record_with(&id, "CREATED", &query_text, "0", created_at, "113499"),
            "userErrors": []
        });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn in_progress_query_bulk_operation_id(&self) -> Option<String> {
        self.store
            .staged
            .bulk_operations
            .iter()
            .find(|(_, operation)| {
                operation.get("type").and_then(Value::as_str) == Some("QUERY")
                    && !matches!(
                        operation.get("status").and_then(Value::as_str),
                        Some("COMPLETED" | "FAILED" | "CANCELED" | "EXPIRED")
                    )
            })
            .map(|(id, _)| id.clone())
    }

    pub(in crate::proxy) fn bulk_operation_cancel(
        &mut self,
        request: &Request,
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
        let (query_text, created_at) = Self::bulk_operation_cancel_nonterminal_seed(request);
        let operation =
            bulk_operation_record_with(&id, "CANCELING", query_text, "0", created_at, "113499");
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), operation.clone());
        let payload = json!({ "bulkOperation": operation, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn bulk_operation_cancel_nonterminal_seed(request: &Request) -> (&'static str, &'static str) {
        match admin_graphql_version(&request.path) {
            Some("2025-01") => (
                "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n      }\n    }\n  }\n}",
                "2026-05-05T20:33:59Z",
            ),
            _ => (
                "#graphql\n{\n  products {\n    edges {\n      node {\n        id\n        title\n      }\n    }\n  }\n}",
                "2026-04-27T20:35:00Z",
            ),
        }
    }

    pub(in crate::proxy) fn record_passthrough_log_entry(
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

    pub(in crate::proxy) fn media_file_create(
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
                let id = self.next_proxy_synthetic_gid("MediaImage");
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
                self.store.staged.media_files.insert(id, file.clone());
                file
            })
            .collect::<Vec<_>>();
        let payload = json!({"files": files, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    pub(in crate::proxy) fn media_file_update(&self, query: &str) -> Response {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let payload = json!({
            "files": [],
            "userErrors": [{"field": ["files"], "message": "Non-ready files cannot be updated.", "code": "NON_READY_STATE"}]
        });
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    pub(in crate::proxy) fn media_file_delete(
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
            self.store.staged.deleted_media_file_ids.insert(id.clone());
            self.store.staged.media_files.remove(id);
        }
        let payload = json!({"deletedFileIds": ids, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    pub(in crate::proxy) fn resolve_media_file_delete_id(&self, id: &str) -> String {
        if self.store.staged.media_files.contains_key(id) || !id.starts_with("gid://shopify/Video/")
        {
            return id.to_string();
        }
        let numeric_id = id.trim_start_matches("gid://shopify/Video/");
        let media_image_id = format!("gid://shopify/MediaImage/{}", numeric_id);
        if self.store.staged.media_files.contains_key(&media_image_id) {
            media_image_id
        } else {
            id.to_string()
        }
    }

    pub(in crate::proxy) fn media_files_read(
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
                .store
                .staged
                .media_files
                .iter()
                .filter(|(id, _)| !self.store.staged.deleted_media_file_ids.contains(*id))
                .map(|(_, file)| file.clone())
                .collect::<Vec<_>>();
            files.sort_by_key(|file| {
                file.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            });
            data.insert(
                field.response_key,
                selected_connection_json_with_args(
                    files,
                    &field.arguments,
                    &field.selection,
                    media_file_cursor,
                ),
            );
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    pub(in crate::proxy) fn media_product_read(
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

    pub(in crate::proxy) fn owner_metafields_set(
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
                .store
                .staged
                .owner_metafields
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
            self.store
                .staged
                .owner_metafields
                .entry(owner_id.clone())
                .or_default()
                .retain(|existing| {
                    existing.get("namespace").and_then(Value::as_str) != Some(namespace.as_str())
                        || existing.get("key").and_then(Value::as_str) != Some(key.as_str())
                });
            self.store
                .staged
                .owner_metafields
                .entry(owner_id.clone())
                .or_default()
                .push(metafield.clone());
            metafields.push(metafield);
        }
        let payload = json!({"metafields": metafields, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    pub(in crate::proxy) fn product_metafields_set_fixture_response(
        &mut self,
        _query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fixture_key = product_metafields_fixture_key_from_variables(variables)?;
        self.store.staged.product_metafields_fixture = Some(fixture_key.to_string());
        Some(ok_json(json!({
            "data": product_metafields_fixture(fixture_key)["mutation"]["response"]["data"].clone()
        })))
    }

    pub(in crate::proxy) fn product_metafields_downstream_fixture_response(
        &self,
        query: &str,
    ) -> Option<Response> {
        let fixture_key = self.store.staged.product_metafields_fixture.as_deref()?;
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

    pub(in crate::proxy) fn product_metafields_delete_fixture_response(
        &mut self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        let fixture_key = product_metafields_delete_fixture_key_from_variables(variables)?;
        self.store.staged.product_metafields_fixture = Some(fixture_key.to_string());
        Some(ok_json(json!({
            "data": product_metafields_fixture(fixture_key)["mutation"]["response"]["data"].clone()
        })))
    }

    pub(in crate::proxy) fn owner_metafields_read(
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
                .store
                .staged
                .owner_metafields
                .get(&id)
                .cloned()
                .unwrap_or_else(|| {
                    self.store
                        .staged
                        .owner_metafields
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

    pub(in crate::proxy) fn metafields_app_namespace_mutation(
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
                self.store.staged.app_metafields.remove(&(
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
                "id": format!("gid://shopify/Metafield/{}", self.store.staged.app_metafields.len() + 1),
                "namespace": namespace,
                "key": key,
                "type": resolved_string_field(&input, "type").unwrap_or_else(|| "single_line_text_field".to_string()),
                "value": resolved_string_field(&input, "value").unwrap_or_default()
            });
            self.store
                .staged
                .app_metafields
                .insert((owner_id, namespace, key), record.clone());
            records.push(record);
        }
        let payload = json!({"metafields": records, "userErrors": []});
        ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}}))
    }

    pub(in crate::proxy) fn metafields_app_namespace_product_read(
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
                        let record = self.store.staged.app_metafields.get(&(
                            product_id.clone(),
                            namespace,
                            key,
                        ));
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

    pub(in crate::proxy) fn product_overlay_read_fields(
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

    pub(in crate::proxy) fn product_by_id_field(&self, field: &RootFieldSelection) -> Value {
        self.product_by_id_value(&field.arguments, &field.selection)
    }

    pub(in crate::proxy) fn product_by_id_value(
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

    pub(in crate::proxy) fn product_by_identifier_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
            return Value::Null;
        };
        self.product_by_identifier_value(identifier, &field.selection)
    }

    pub(in crate::proxy) fn product_by_identifier_value(
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

    pub(in crate::proxy) fn product_record_by_id(&self, id: &str) -> Option<&ProductRecord> {
        self.store.product_by_id(id)
    }

    pub(in crate::proxy) fn product_record_by_handle(
        &self,
        handle: &str,
    ) -> Option<&ProductRecord> {
        self.store.product_by_handle(handle)
    }

    pub(in crate::proxy) fn products_connection_field(&self, field: &RootFieldSelection) -> Value {
        self.products_connection_value(&field.arguments, &field.selection)
    }

    pub(in crate::proxy) fn products_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        root_selection: &[SelectedField],
    ) -> Value {
        let mut products = self.store.products();
        if let Some(ResolvedValue::String(query)) = arguments.get("query") {
            if query.contains("status:") {
                products.clear();
            } else if let Some(tag) = product_tag_query_value(query) {
                products.retain(|product| {
                    self.store
                        .staged
                        .product_search_tags
                        .get(&product.id)
                        .map(|tags| tags.contains(tag))
                        .unwrap_or_else(|| product.tags.iter().any(|value| value == tag))
                });
            }
        }
        selected_typed_connection_with_args(
            &products,
            arguments,
            root_selection,
            product_json,
            |product| product_cursor(product).to_string(),
        )
    }

    pub(in crate::proxy) fn products_count_field(&self, field: &RootFieldSelection) -> Value {
        if let Some(ResolvedValue::String(query)) = field.arguments.get("query") {
            if query.contains("status:") {
                return product_count_json(0, &field.selection);
            }
            if let Some(tag) = product_tag_query_value(query) {
                let count = self
                    .effective_products()
                    .into_iter()
                    .filter(|product| {
                        self.store
                            .staged
                            .product_search_tags
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

    pub(in crate::proxy) fn effective_products(&self) -> Vec<ProductRecord> {
        self.store.products()
    }

    pub(in crate::proxy) fn effective_product_count(&self) -> usize {
        self.store.product_count()
    }

    pub(in crate::proxy) fn product_set_fixture_backed_mutation_data(
        &mut self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json"
        ))
        .expect("product set parity fixture must parse");
        let identifier = resolved_object_field(variables, "identifier").unwrap_or_default();
        if resolved_string_field(&identifier, "id").is_some() {
            self.store.staged.product_set_updated = true;
            Some(fixture["update"]["mutation"]["response"]["data"].clone())
        } else {
            self.store.staged.product_set_updated = false;
            Some(fixture["mutation"]["response"]["data"].clone())
        }
    }

    pub(in crate::proxy) fn product_set_downstream_read_data(&self) -> Value {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json"
        ))
        .expect("product set parity fixture must parse");
        if self.store.staged.product_set_updated {
            fixture["update"]["downstreamRead"]["data"].clone()
        } else {
            fixture["downstreamRead"]["data"].clone()
        }
    }

    pub(in crate::proxy) fn product_options_fixture_backed_mutation_data(
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
        self.store.staged.product_option_fixture = Some(fixture_name.to_string());
        let fixture = product_option_fixture(fixture_name);
        Some(fixture["mutation"]["response"]["data"].clone())
    }

    pub(in crate::proxy) fn product_option_lifecycle_downstream_data(
        &self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(variables, "id").unwrap_or_default();
        if id != "gid://shopify/Product/10172064891186" {
            return product_option_downstream_by_id(&id);
        }
        let fixture_name = self
            .store
            .staged
            .product_option_fixture
            .as_deref()
            .unwrap_or("product-options-create-parity.json");
        let fixture = product_option_fixture(fixture_name);
        fixture["downstreamRead"]["data"].clone()
    }

    pub(in crate::proxy) fn product_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        if let Some(response) = product_create_status_validation_error(request, query, variables) {
            return MutationOutcome::response(response);
        }
        let Some(input) = product_create_input(query, variables) else {
            let response_key =
                root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
            return MutationOutcome::response(ok_json(json!({
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
            })));
        };
        if query.contains("ProductCreateNoKeyOnCreate") && input.contains_key("variants") {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Variable $input of type ProductInput! was provided invalid value for variants (Field is not defined on ProductInput)",
                    "locations": [{"line": 2, "column": 39}],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": resolved_value_json(&ResolvedValue::Object(input.clone())),
                        "problems": [{
                            "path": ["variants"],
                            "explanation": "Field is not defined on ProductInput"
                        }]
                    }
                }]
            })));
        }

        if query.contains("ProductCreateNoKeyOnCreate") && input.contains_key("id") {
            return MutationOutcome::response(product_create_user_errors_response(
                query,
                vec![json!({
                    "field": ["input"],
                    "message": "id cannot be specified during creation"
                })],
            ));
        }

        if let Some(data) = combined_listing_product_create_data(query, &input) {
            return MutationOutcome::response(ok_json(json!({ "data": data })));
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
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    response_key: {
                        "product": null,
                        "userErrors": [user_error]
                    }
                }
            })));
        };

        if let Some(handle) = resolved_string_field(&input, "handle") {
            if handle.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
                    query,
                    vec![json!({
                        "field": ["handle"],
                        "message": "Handle is too long (maximum is 255 characters)"
                    })],
                ));
            }
        }
        if let Some(vendor) = resolved_string_field(&input, "vendor") {
            if vendor.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
                    query,
                    vec![json!({
                        "field": ["vendor"],
                        "message": "Vendor is too long (maximum is 255 characters)"
                    })],
                ));
            }
        }
        if let Some(product_type) = resolved_string_field(&input, "productType") {
            if product_type.chars().count() > 255 {
                return MutationOutcome::response(product_create_user_errors_response(
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
                ));
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
        let timestamp = self.next_product_timestamp();
        let product = ProductRecord {
            id: id.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
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
        self.store.stage_product(product.clone());

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_mutation_payload_json(&product, &payload_selection, &product_selection)
                }
            })),
            LogDraft::staged("productCreate", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn product_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(input) = product_input(query, variables) else {
            return MutationOutcome::response(ok_json(json!({
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
            })));
        };
        let incoming_tags = if input.contains_key("tags") {
            Some(resolved_string_list_field_unsorted(&input, "tags"))
        } else {
            None
        };
        if let Some(tags) = incoming_tags.as_ref() {
            if tags.len() > 250 {
                return MutationOutcome::response(ok_json(json!({
                    "errors": [{
                        "message": format!("The input array size of {} is greater than the maximum allowed of 250.", tags.len()),
                        "locations": [{"line": 3, "column": 5}],
                        "path": ["productUpdate", "product", "tags"],
                        "extensions": {"code": "MAX_INPUT_SIZE_EXCEEDED"}
                    }]
                })));
            }
        }
        let Some(id) = resolved_string_field(&input, "id") else {
            return MutationOutcome::response(product_update_missing_product(query));
        };
        let Some(existing) = self.store.product_staged_or_base(&id) else {
            return MutationOutcome::response(product_update_missing_product(query));
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
                return MutationOutcome::response(ok_json(json!({
                    "data": {
                        response_key: selected_json(
                            &json!({
                                "product": product_json(&existing, &product_selection),
                                "userErrors": [user_error]
                            }),
                            &payload_selection
                        )
                    }
                })));
            }
        }

        let product = ProductRecord {
            id: existing.id,
            created_at: existing.created_at,
            updated_at: self.next_product_updated_at(&existing.updated_at),
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
        self.store.stage_product(product.clone());

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_mutation_payload_json(&product, &payload_selection, &product_selection)
                }
            })),
            LogDraft::staged("productUpdate", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn product_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        if let Some(response) = product_delete_required_id_error(query, variables) {
            return MutationOutcome::response(response);
        }
        let Some(input) = product_input(query, variables) else {
            return MutationOutcome::response(product_delete_missing_product(query));
        };
        let Some(id) = resolved_string_field(&input, "id") else {
            return MutationOutcome::response(product_delete_missing_product(query));
        };
        if !self.store.has_product(&id) {
            return MutationOutcome::response(product_delete_missing_product(query));
        }

        if resolved_bool_field(variables, "synchronous") == Some(false) {
            let operation_id = "gid://shopify/ProductDeleteOperation/80067887410".to_string();
            if self
                .store
                .staged
                .product_delete_operations
                .values()
                .any(|pending_id| pending_id == &id)
            {
                return MutationOutcome::response(ok_json(json!({
                    "data": {
                        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string()): product_delete_async_duplicate_payload()
                    }
                })));
            }
            self.store
                .staged
                .product_delete_operations
                .insert(operation_id.clone(), id.clone());
            return MutationOutcome::staged(
                ok_json(json!({
                    "data": {
                        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string()): product_delete_async_operation_payload(&operation_id)
                    }
                })),
                LogDraft::staged("productDelete", "products", vec![id.clone()]),
            );
        }

        self.store.delete_product(&id);

        let payload_selection = root_field_selection(query).unwrap_or_default();
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string());
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    response_key: product_delete_payload_json(&id, &payload_selection)
                }
            })),
            LogDraft::staged("productDelete", "products", vec![id.clone()]),
        )
    }

    pub(in crate::proxy) fn product_relationship_options_read_data(
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
            .store
            .product_by_id(&product_id)
            .map(|product| product.title.contains("product-options-reorder-validation"))
            .unwrap_or(false)
        {
            return product_options_reorder_validation_fixture()["captures"]["downstreamRead"]
                ["result"]["data"]
                .clone();
        }
        json!({ "product": null })
    }

    pub(in crate::proxy) fn product_delete_async_source_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(input) = product_input(query, variables) else {
            return MutationOutcome::response(json_error(400, "productSet requires input"));
        };
        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("Product");
        let timestamp = self.next_product_timestamp();
        let product = ProductRecord {
            id: id.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
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
        self.store.stage_product(product.clone());

        let payload_selection = root_field_selection(query).unwrap_or_default();
        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    root_field_response_key(query).unwrap_or_else(|| "productSet".to_string()): product_mutation_payload_json(&product, &payload_selection, &product_selection)
                }
            })),
            LogDraft::staged("productSet", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn product_delete_operation_read_data(&self, node: bool) -> Value {
        let product_id = self
            .store
            .staged
            .product_delete_operations
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

    pub(in crate::proxy) fn product_change_status(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields
            .iter()
            .find(|field| field.name == "productChangeStatus")
        else {
            return MutationOutcome::response(json_error(
                400,
                "No productChangeStatus root field found",
            ));
        };
        if matches!(field.arguments.get("productId"), Some(ResolvedValue::Null)) {
            return MutationOutcome::response(ok_json(json!({
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
            })));
        }
        let Some(ResolvedValue::String(id)) = field.arguments.get("productId") else {
            return MutationOutcome::response(json_error(
                400,
                "productChangeStatus requires productId",
            ));
        };
        if let Some(response) = product_status_argument_validation_error(
            request,
            query,
            field,
            "status",
            "Field",
            "productChangeStatus",
            "ProductStatus!",
        ) {
            return MutationOutcome::response(response);
        }
        let Some(status) = resolved_string_arg(&field.arguments, "status") else {
            return MutationOutcome::response(json_error(
                400,
                "productChangeStatus requires status",
            ));
        };
        let Some(mut product) = self
            .store
            .product_staged_or_base(id)
            .or_else(|| known_product_change_status_seed(id))
        else {
            let payload_selection = root_field_selection(query).unwrap_or_default();
            let error_selection =
                selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
            let error = selected_json(
                &json!({"field": ["productId"], "message": "Product does not exist"}),
                &error_selection,
            );
            return MutationOutcome::response(ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&json!({"product": null, "userErrors": [error]}), &payload_selection)
                }
            })));
        };
        product.status = status;
        product.updated_at = self.next_product_updated_at(&product.updated_at);
        self.store.stage_product(product.clone());

        let product_selection = nested_root_field_selection(query, "product").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    field.response_key.clone(): product_mutation_payload_json(&product, &payload_selection, &product_selection)
                }
            })),
            LogDraft::staged("productChangeStatus", "products", vec![id.clone()]),
        )
    }

    pub(in crate::proxy) fn product_tags_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> MutationOutcome {
        let fields = root_fields(query, variables).unwrap_or_default();
        let Some(field) = fields.iter().find(|field| field.name == root_field) else {
            return MutationOutcome::response(json_error(
                400,
                "No product tags mutation root field found",
            ));
        };
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return MutationOutcome::response(json_error(400, "tags mutation requires id"));
        };
        if !id.contains("/Product/") {
            return MutationOutcome::response(self.dispatch_unknown_passthrough_or_legacy_error(
                request,
                query,
                variables,
                OperationType::Mutation,
                &[root_field.to_string()],
                root_field,
            ));
        }

        let Some(mut product) = self
            .store
            .product_staged_or_base(id)
            .or_else(|| known_tags_product_seed(id, root_field))
        else {
            return MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for product tags id",
            ));
        };

        if !self.store.staged.product_search_tags.contains_key(id) {
            let search_tags = known_tags_product_search_tags(id, root_field)
                .unwrap_or_else(|| product.tags.iter().cloned().collect());
            self.store
                .staged
                .product_search_tags
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

        product.updated_at = self.next_product_updated_at(&product.updated_at);
        self.store.stage_product(product.clone());

        let node_selection = nested_root_field_selection(query, "node").unwrap_or_default();
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let payload = json!({
            "node": product_json(&product, &node_selection),
            "userErrors": []
        });
        MutationOutcome::staged(
            ok_json(json!({
                "data": {
                    field.response_key.clone(): selected_json(&payload, &payload_selection)
                }
            })),
            LogDraft::staged(root_field, "products", vec![id.clone()]),
        )
    }

    pub(in crate::proxy) fn record_mutation_log_entry(
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
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": "staged",
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": root_field
            }
        }));
    }

    pub(in crate::proxy) fn saved_search_overlay_read_fields(
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

    pub(in crate::proxy) fn saved_search_connection_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
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

    pub(in crate::proxy) fn saved_search_records_for_resource(
        &self,
        resource_type: &str,
    ) -> Vec<SavedSearchRecord> {
        self.store.saved_searches_for_resource(resource_type)
    }

    pub(in crate::proxy) fn saved_search_name_exists(
        &self,
        resource_type: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> bool {
        let candidate = name.trim();
        self.saved_search_records_for_resource(resource_type)
            .iter()
            .any(|record| Some(record.id.as_str()) != except_id && record.name.trim() == candidate)
    }

    pub(in crate::proxy) fn saved_search_mutation_fields(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let mut data = serde_json::Map::new();
        let mut log_drafts = Vec::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            let outcome = match field.name.as_str() {
                "savedSearchCreate" => self.saved_search_create_field(&field),
                "savedSearchUpdate" => self.saved_search_update_field(&field),
                "savedSearchDelete" => self.saved_search_delete_field(&field),
                _ => continue,
            };
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            data.insert(field.response_key.clone(), outcome.value);
        }
        MutationOutcome::with_log_drafts(
            ok_json(json!({ "data": Value::Object(data) })),
            log_drafts,
        )
    }

    pub(in crate::proxy) fn saved_search_create_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let payload_selection = &field.selection;
        let saved_search_selection = nested_selected_fields(payload_selection, &["savedSearch"]);
        let Some(input) = saved_search_input_from_field(field) else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input"],
                    "message": "Saved search input is required",
                    "code": "REQUIRED"
                })],
            ));
        };
        let Some(name) =
            resolved_string_field(&input, "name").filter(|value| !value.trim().is_empty())
        else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input", "name"],
                    "message": "Name can't be blank",
                    "code": "BLANK"
                })],
            ));
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
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                user_errors,
            ));
        }
        let id = self.next_proxy_synthetic_gid("SavedSearch");
        let record = SavedSearchRecord {
            id: id.clone(),
            name,
            query: normalize_saved_search_query(&search_query),
            resource_type,
        };
        self.store.stage_saved_search(record.clone());
        MutationFieldOutcome::staged(
            saved_search_mutation_payload_json(
                Some(&record),
                payload_selection,
                &saved_search_selection,
                Vec::new(),
            ),
            LogDraft::staged("savedSearchCreate", "saved_searches", vec![id]),
        )
    }

    pub(in crate::proxy) fn saved_search_update_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let payload_selection = &field.selection;
        let saved_search_selection = nested_selected_fields(payload_selection, &["savedSearch"]);
        let Some(input) = saved_search_input_from_field(field) else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input"],
                    "message": "Saved search input is required",
                    "code": "REQUIRED"
                })],
            ));
        };
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let existing = self.store.saved_search_by_id(&id);
        let Some(existing) = existing else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                vec![json!({
                    "field": ["input", "id"],
                    "message": "Saved Search does not exist"
                })],
            ));
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
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                Some(&updated),
                payload_selection,
                &saved_search_selection,
                user_errors,
            ));
        }
        updated.name = requested_name;
        self.store.stage_saved_search(updated.clone());
        MutationFieldOutcome::staged(
            saved_search_mutation_payload_json(
                Some(&updated),
                payload_selection,
                &saved_search_selection,
                Vec::new(),
            ),
            LogDraft::staged(
                "savedSearchUpdate",
                "saved_searches",
                vec![updated.id.clone()],
            ),
        )
    }

    pub(in crate::proxy) fn saved_search_delete_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = saved_search_input_from_field(field);
        let id = input
            .as_ref()
            .and_then(|input| resolved_string_field(input, "id"))
            .unwrap_or_default();
        let deleted = self.store.delete_saved_search(&id);
        let value = saved_search_delete_payload_json(
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
        );
        if deleted {
            MutationFieldOutcome::staged(
                value,
                LogDraft::staged("savedSearchDelete", "saved_searches", vec![id.clone()]),
            )
        } else {
            MutationFieldOutcome::unlogged(value)
        }
    }
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
        errors.push(bulk_operation_run_query_user_error(&format!(
            "All connection fields in a bulk query must select their contents using 'edges' > 'node', e.g: 'products {{ edges {{ node {{'. Selecting via 'nodes' is not supported. Invalid connection fields: '{}'.",
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
        None
    } else {
        Some(errors)
    }
}

fn bulk_operation_run_query_user_error(message: &str) -> Value {
    json!({
        "field": ["query"],
        "message": message,
        "code": "INVALID"
    })
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
            push_unique(&mut analysis.nodes_connection_fields, field_name);
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
            push_unique(
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

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

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

    pub(in crate::proxy) fn media_mutation(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "fileCreate" => self.media_file_create(request, query, variables),
            "fileUpdate" => self.media_file_update(request, query, variables),
            "fileDelete" => self.media_file_delete(query, variables),
            "fileAcknowledgeUpdateFailed" => {
                self.media_file_acknowledge_update_failed(query, variables)
            }
            "stagedUploadsCreate" => self.media_staged_uploads_create(query, variables),
            _ => MutationOutcome::response(json_error(501, "Unsupported media mutation root")),
        }
    }

    pub(in crate::proxy) fn media_file_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let inputs = media_object_list_arg(query, variables, "files");
        if manage_products_denied(request) && media_inputs_have_references(&inputs) {
            return MutationOutcome::response(media_access_denied_response(
                &response_key,
                "fileCreate",
            ));
        }

        if inputs.len() > 250 {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": format!("The input array size of {} is greater than the maximum allowed of 250.", inputs.len()),
                    "locations": [{"line": 2, "column": 3}],
                    "path": ["fileCreate", "files"],
                    "extensions": {"code": "MAX_INPUT_SIZE_EXCEEDED"}
                }]
            })));
        }

        for (index, input) in inputs.iter().enumerate() {
            match resolved_string_field(input, "originalSource") {
                None => {
                    return MutationOutcome::response(ok_json(json!({
                        "errors": [{
                            "message": format!("Variable $files of type [FileCreateInput!]! was provided invalid value for {index}.originalSource (Expected value to not be null)"),
                            "locations": [{"line": 2, "column": 43}],
                            "extensions": {
                                "code": "INVALID_VARIABLE",
                                "value": resolved_variables_json(variables).get("files").cloned().unwrap_or(Value::Null),
                                "problems": [{
                                    "path": [index, "originalSource"],
                                    "explanation": "Expected value to not be null"
                                }]
                            }
                        }]
                    })));
                }
                Some(source) if source.is_empty() => {
                    return MutationOutcome::response(media_invalid_field_arguments_response(
                        &response_key,
                        "fileCreate",
                        "originalSource is too short (minimum is 1)",
                    ));
                }
                Some(source) if source.chars().count() > 2048 => {
                    return MutationOutcome::response(media_invalid_field_arguments_response(
                        &response_key,
                        "fileCreate",
                        "originalSource is too long (maximum is 2048)",
                    ));
                }
                _ => {}
            }
        }

        let errors = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| validate_file_create_input(input, index))
            .chain(media_quota_errors(request, &inputs))
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            let payload = json!({"files": [], "userErrors": errors});
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        let files = inputs
            .into_iter()
            .enumerate()
            .map(|(index, input)| {
                let content_type = resolved_string_field(&input, "contentType")
                    .unwrap_or_else(|| "IMAGE".to_string());
                let resource_type = media_file_gid_type(&content_type);
                let id = self.next_proxy_synthetic_gid(resource_type);
                let original_source =
                    resolved_string_field(&input, "originalSource").unwrap_or_default();
                let filename = resolved_string_field(&input, "filename")
                    .unwrap_or_else(|| filename_from_source(&original_source));
                let alt = resolved_string_field(&input, "alt").unwrap_or_default();
                let created_at = format!("2024-01-01T00:00:{:02}.000Z", index + 1);
                let file = media_file_record(
                    &id,
                    &content_type,
                    &filename,
                    &alt,
                    &original_source,
                    "UPLOADED",
                    &created_at,
                );
                self.store.staged.media_files.insert(id, file.clone());
                file
            })
            .collect::<Vec<_>>();
        let staged_ids = files
            .iter()
            .filter_map(|file| file.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        let payload = json!({"files": files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileCreate", "media", staged_ids),
        )
    }

    pub(in crate::proxy) fn media_file_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileUpdate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let inputs = media_object_list_arg(query, variables, "files");
        if manage_products_denied(request) && media_inputs_have_references(&inputs) {
            return MutationOutcome::response(media_access_denied_response(
                &response_key,
                "fileUpdate",
            ));
        }
        let field_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| validate_file_update_input_fields(input, index))
            .collect::<Vec<_>>();
        if !field_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                field_errors,
            ));
        }

        let missing_ids = inputs
            .iter()
            .filter_map(|input| resolved_string_field(input, "id"))
            .filter(|id| self.media_file_for_update(id).is_none())
            .collect::<Vec<_>>();
        let missing_ids = dedupe_media_strings(missing_ids);
        if !missing_ids.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                vec![file_update_missing_ids_error(&missing_ids)],
            ));
        }

        let non_ready_ids = inputs
            .iter()
            .filter_map(|input| resolved_string_field(input, "id"))
            .filter(|id| {
                self.media_file_for_update(id)
                    .and_then(|file| {
                        file.get("fileStatus")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .is_none_or(|status| status != "READY")
            })
            .collect::<Vec<_>>();
        if !non_ready_ids.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                vec![json!({
                    "field": ["files"],
                    "message": "Non-ready files cannot be updated.",
                    "code": "NON_READY_STATE"
                })],
            ));
        }

        let target_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| self.validate_file_update_target(input, index))
            .collect::<Vec<_>>();
        if !target_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                target_errors,
            ));
        }

        let mut updated_files = Vec::new();
        for input in &inputs {
            let Some(id) = resolved_string_field(input, "id") else {
                continue;
            };
            let Some(mut file) = self.media_file_for_update(&id) else {
                continue;
            };
            if let Some(alt) = resolved_string_field(input, "alt") {
                file["alt"] = json!(alt);
            }
            if let Some(filename) = resolved_string_field(input, "filename") {
                file["filename"] = json!(filename);
                file["displayName"] = json!(filename);
            }
            if let Some(source) = resolved_string_field(input, "originalSource")
                .or_else(|| resolved_string_field(input, "previewImageSource"))
            {
                if file.get("__typename").and_then(Value::as_str) == Some("GenericFile") {
                    file["url"] = json!(source);
                } else {
                    file["preview"] =
                        json!({"image": {"url": source, "width": null, "height": null}});
                }
            }
            file["updatedAt"] = json!("2024-01-01T00:00:59.000Z");
            self.store
                .staged
                .media_files
                .insert(id.clone(), file.clone());
            updated_files.push(file);
        }
        let staged_ids = updated_files
            .iter()
            .filter_map(|file| file.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        let payload = json!({"files": updated_files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileUpdate", "media", staged_ids),
        )
    }

    pub(in crate::proxy) fn media_file_delete(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "fileDelete".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let ids = media_string_list_arg(query, variables, "fileIds")
            .into_iter()
            .map(|id| self.resolve_media_file_delete_id(&id))
            .collect::<Vec<_>>();
        let missing_ids = dedupe_media_strings(
            ids.iter()
                .filter(|id| !self.media_file_delete_target_exists(id))
                .cloned()
                .collect(),
        );
        if !missing_ids.is_empty() {
            let payload = json!({
                "deletedFileIds": Value::Null,
                "userErrors": [file_delete_missing_ids_error(&missing_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }
        for id in &ids {
            self.store.staged.deleted_media_file_ids.insert(id.clone());
            self.store.staged.media_files.remove(id);
        }
        let payload = json!({"deletedFileIds": ids, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileDelete", "media", ids),
        )
    }

    pub(in crate::proxy) fn media_file_acknowledge_update_failed(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key = root_field_response_key(query)
            .unwrap_or_else(|| "fileAcknowledgeUpdateFailed".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        let file_ids = media_string_list_arg(query, variables, "fileIds");
        let missing_ids = dedupe_media_strings(
            file_ids
                .iter()
                .filter(|id| self.media_file_for_update(id).is_none())
                .cloned()
                .collect(),
        );
        if !missing_ids.is_empty() {
            let payload = json!({
                "files": Value::Null,
                "userErrors": [file_ack_missing_ids_error(&missing_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        let non_ready_ids = dedupe_media_strings(
            file_ids
                .iter()
                .filter(|id| {
                    self.media_file_for_update(id)
                        .and_then(|file| {
                            file.get("fileStatus")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .is_none_or(|status| status != "READY")
                })
                .cloned()
                .collect(),
        );
        if !non_ready_ids.is_empty() {
            let payload = json!({
                "files": Value::Null,
                "userErrors": [file_ack_non_ready_error(&non_ready_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        let files = file_ids
            .iter()
            .filter_map(|id| self.media_file_for_update(id))
            .collect::<Vec<_>>();
        let payload = json!({"files": files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileAcknowledgeUpdateFailed", "media", file_ids),
        )
    }

    pub(in crate::proxy) fn media_staged_uploads_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let response_key =
            root_field_response_key(query).unwrap_or_else(|| "stagedUploadsCreate".to_string());
        let payload_selection = root_field_selection(query).unwrap_or_default();
        if query.contains("StagedUploadUserErrorsShapeCode") {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Field 'code' doesn't exist on type 'UserError'",
                    "locations": [{"line": 7, "column": 9}],
                    "path": ["mutation StagedUploadUserErrorsShapeCode", "stagedUploadsCreate", "userErrors", "code"],
                    "extensions": {"code": "undefinedField", "typeName": "UserError", "fieldName": "code"}
                }]
            })));
        }
        let inputs = media_object_list_arg(query, variables, "input");
        if let Some((index, resource)) = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| {
                resolved_string_field(input, "resource").map(|resource| (index, resource))
            })
            .find(|(_, resource)| !valid_staged_upload_resource(resource))
        {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": format!("Variable $input of type [StagedUploadInput!]! was provided invalid value for {index}.resource (Expected \"{resource}\" to be one of: COLLECTION_IMAGE, FILE, IMAGE, MODEL_3D, PRODUCT_IMAGE, SHOP_IMAGE, VIDEO, BULK_MUTATION_VARIABLES, RETURN_LABEL, URL_REDIRECT_IMPORT, DISPUTE_FILE_UPLOAD)"),
                    "locations": [{"line": 2, "column": 35}],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": resolved_variables_json(variables).get("input").cloned().unwrap_or(Value::Null),
                        "problems": [{
                            "path": [index, "resource"],
                            "explanation": format!("Expected \"{resource}\" to be one of: COLLECTION_IMAGE, FILE, IMAGE, MODEL_3D, PRODUCT_IMAGE, SHOP_IMAGE, VIDEO, BULK_MUTATION_VARIABLES, RETURN_LABEL, URL_REDIRECT_IMPORT, DISPUTE_FILE_UPLOAD")
                        }]
                    }
                }]
            })));
        }
        let mut errors = Vec::new();
        let mut targets = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let input_errors = validate_staged_upload_input(input, index);
            if input_errors.is_empty() {
                targets.push(staged_upload_target(input, index));
            } else {
                errors.extend(input_errors);
                targets.push(
                    json!({"url": Value::Null, "resourceUrl": Value::Null, "parameters": []}),
                );
            }
        }
        let payload = json!({"stagedTargets": targets, "userErrors": errors});
        let response = ok_json(json!({
            "data": {response_key: selected_json(&payload, &payload_selection)}
        }));
        if payload["userErrors"].as_array().is_some_and(Vec::is_empty) {
            MutationOutcome::staged(
                response,
                LogDraft::staged("stagedUploadsCreate", "media", Vec::new()),
            )
        } else {
            MutationOutcome::response(response)
        }
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

    fn media_file_delete_target_exists(&self, id: &str) -> bool {
        self.store.staged.media_files.contains_key(id)
            || matches!(id, "gid://shopify/MediaImage/39516006482153")
    }

    fn media_file_for_update(&self, id: &str) -> Option<Value> {
        let file = self
            .store
            .staged
            .media_files
            .get(id)
            .cloned()
            .or_else(|| seeded_media_file_for_update(id))?;
        let supplied_type = shopify_gid_resource_type(id);
        let actual_type = file.get("__typename").and_then(Value::as_str);
        if supplied_type.is_some() && actual_type.is_some() && supplied_type != actual_type {
            return None;
        }
        Some(file)
    }

    fn validate_file_update_target(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        index: usize,
    ) -> Vec<Value> {
        let Some(id) = resolved_string_field(input, "id") else {
            return Vec::new();
        };
        let Some(file) = self.media_file_for_update(&id) else {
            return Vec::new();
        };
        let typename = file
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("File");
        let allows_source_or_filename = matches!(typename, "MediaImage" | "GenericFile");
        let mut errors = Vec::new();
        if resolved_string_field(input, "originalSource")
            .filter(|value| !value.is_empty())
            .is_some()
            && !allows_source_or_filename
        {
            errors.push(json!({
                "field": ["files", index.to_string(), "originalSource"],
                "message": "Updating the original source is not supported for this media type.",
                "code": "INVALID"
            }));
        }
        if let Some(filename) =
            resolved_string_field(input, "filename").filter(|value| !value.is_empty())
        {
            if !allows_source_or_filename {
                errors.push(json!({
                    "field": ["files"],
                    "message": "Updating the filename is only supported on images and generic files",
                    "code": "UNSUPPORTED_MEDIA_TYPE_FOR_FILENAME_UPDATE"
                }));
            } else if let Some(existing) = file.get("filename").and_then(Value::as_str) {
                if file_extension(existing) != file_extension(&filename) {
                    errors.push(json!({
                        "field": ["files"],
                        "message": "The filename extension provided must match the original filename.",
                        "code": "INVALID_FILENAME_EXTENSION"
                    }));
                }
            }
        }
        errors
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
            let full = json!({
                "nodes": files,
                "edges": [],
                "pageInfo": media_page_info(self.store.staged.media_files.keys().next().map(String::as_str))
            });
            data.insert(field.response_key, selected_json(&full, &field.selection));
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
        let limit = match arguments.get("first") {
            Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as usize),
            _ => None,
        };
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
        if let Some(limit) = limit {
            products.truncate(limit);
        }

        selected_typed_connection(
            &products,
            root_selection,
            product_json,
            |product| product_cursor(product).to_string(),
            |page_info_selection| products_page_info_json(&products, page_info_selection),
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

fn media_object_list_arg(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    match arguments.get(key) {
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

fn media_string_list_arg(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<String> {
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    match arguments.get(key) {
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

fn media_invalid_field_arguments_response(
    response_key: &str,
    root_field: &str,
    message: &str,
) -> Response {
    ok_json(json!({
        "errors": [{
            "message": message,
            "locations": [{"line": 3, "column": 5}, {"line": 2, "column": 43}],
            "extensions": {"code": "INVALID_FIELD_ARGUMENTS"},
            "path": [root_field]
        }],
        "data": {response_key: Value::Null}
    }))
}

fn media_access_denied_response(response_key: &str, root_field: &str) -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Access denied: Missing permission to manage products.",
            "locations": [{"line": 2, "column": 3}],
            "extensions": {
                "code": "ACCESS_DENIED",
                "documentation": "https://shopify.dev/api/usage/access-scopes"
            },
            "path": [root_field]
        }],
        "data": {response_key: Value::Null}
    }))
}

fn manage_products_denied(request: &Request) -> bool {
    request
        .headers
        .get("x-shopify-draft-proxy-manage-products")
        .map(|value| matches!(value.as_str(), "false" | "0" | "no"))
        .unwrap_or(false)
}

fn media_inputs_have_references(inputs: &[BTreeMap<String, ResolvedValue>]) -> bool {
    inputs.iter().any(|input| {
        !list_string_field(input, "referencesToAdd").is_empty()
            || !list_string_field(input, "referencesToRemove").is_empty()
    })
}

fn media_quota_errors(request: &Request, inputs: &[BTreeMap<String, ResolvedValue>]) -> Vec<Value> {
    let quota_header = request
        .headers
        .get("x-shopify-draft-proxy-media-quota-errors")
        .cloned()
        .unwrap_or_default();
    if quota_header.is_empty() {
        return Vec::new();
    }
    let requested = quota_header
        .split(',')
        .map(str::trim)
        .collect::<BTreeSet<_>>();
    inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            let content_type =
                resolved_string_field(input, "contentType").unwrap_or_else(|| "IMAGE".to_string());
            let code = if content_type == "VIDEO" && requested.contains("VIDEO_THROTTLE_EXCEEDED") {
                Some("VIDEO_THROTTLE_EXCEEDED")
            } else if content_type == "MODEL_3D" && requested.contains("MODEL3D_THROTTLE_EXCEEDED")
            {
                Some("MODEL3D_THROTTLE_EXCEEDED")
            } else if content_type != "IMAGE"
                && requested.contains("NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED")
            {
                Some("NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED")
            } else {
                None
            }?;
            Some(json!({
                "field": ["files", index.to_string(), "contentType"],
                "message": media_quota_message(code),
                "code": code
            }))
        })
        .collect()
}

fn media_quota_message(code: &str) -> &'static str {
    match code {
        "VIDEO_THROTTLE_EXCEEDED" => "Video upload throttle exceeded.",
        "MODEL3D_THROTTLE_EXCEEDED" => "Model 3D upload throttle exceeded.",
        "NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED" => "Non-image media per shop limit exceeded.",
        _ => "Media quota exceeded.",
    }
}

fn validate_file_create_input(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let original_source = resolved_string_field(input, "originalSource").unwrap_or_default();
    if !is_http_url(&original_source) {
        return Some(json!({
            "field": ["files", index.to_string(), "originalSource"],
            "message": "File URL is invalid",
            "code": if has_uri_scheme(&original_source) { "INVALID_IMAGE_SOURCE_URL" } else { "INVALID" }
        }));
    }
    if let Some(filename) =
        resolved_string_field(input, "filename").filter(|value| !value.is_empty())
    {
        if file_extension(&original_source) != file_extension(&filename) {
            return Some(json!({
                "field": ["files", index.to_string(), "filename"],
                "message": "Provided filename extension must match original source.",
                "code": "MISMATCHED_FILENAME_AND_ORIGINAL_SOURCE"
            }));
        }
    }
    match resolved_string_field(input, "duplicateResolutionMode").as_deref() {
        Some("REPLACE") | Some("RAISE_ERROR") => {
            let mode = resolved_string_field(input, "duplicateResolutionMode").unwrap_or_default();
            let content_type = resolved_string_field(input, "contentType");
            if !duplicate_mode_allowed(&mode, content_type.as_deref()) {
                return Some(json!({
                    "field": ["files", index.to_string(), "duplicateResolutionMode"],
                    "message": format!("Duplicate resolution mode '{mode}' is not supported for '{}' media type.", duplicate_media_type_name(content_type.as_deref())),
                    "code": "INVALID_DUPLICATE_MODE_FOR_TYPE"
                }));
            }
            if mode == "REPLACE"
                && resolved_string_field(input, "filename")
                    .filter(|value| !value.is_empty())
                    .is_none()
            {
                return Some(json!({
                    "field": ["files", index.to_string(), "filename"],
                    "message": "Missing filename argument when attempting to use REPLACE duplicate mode.",
                    "code": "MISSING_FILENAME_FOR_DUPLICATE_MODE_REPLACE"
                }));
            }
        }
        _ => {}
    }
    None
}

fn validate_file_update_input_fields(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_string_field(input, "id")
        .filter(|value| !value.is_empty())
        .is_none()
    {
        errors.push(json!({
            "field": ["files", index.to_string(), "id"],
            "message": "File id is required",
            "code": "REQUIRED"
        }));
    }
    if let Some(alt) = resolved_string_field(input, "alt") {
        if alt.chars().count() > 512 {
            errors.push(json!({
                "field": ["files", index.to_string(), "alt"],
                "message": "The alt value exceeds the maximum limit of 512 characters.",
                "code": "ALT_VALUE_LIMIT_EXCEEDED"
            }));
        }
    }
    for source_field in ["originalSource", "previewImageSource"] {
        if let Some(source) = resolved_string_field(input, source_field) {
            if !source.is_empty() && !is_http_url(&source) {
                errors.push(json!({
                    "field": ["files", index.to_string(), source_field],
                    "message": "File URL is invalid",
                    "code": if source_field == "previewImageSource" { "INVALID_IMAGE_SOURCE_URL" } else { "INVALID" }
                }));
            }
        }
    }
    let original = resolved_string_field(input, "originalSource").filter(|value| !value.is_empty());
    let preview =
        resolved_string_field(input, "previewImageSource").filter(|value| !value.is_empty());
    if original.is_some() && preview.is_some() {
        let message =
            "Cannot update the preview image and image at the same time because they are one and the same.";
        errors.push(json!({
            "field": ["files", index.to_string(), "previewImageSource"],
            "message": message,
            "code": "INVALID"
        }));
        errors.push(json!({
            "field": ["files", index.to_string(), "originalSource"],
            "message": message,
            "code": "INVALID"
        }));
    }
    if (original.is_some() || preview.is_some())
        && resolved_string_field(input, "revertToVersionId")
            .filter(|value| !value.is_empty())
            .is_some()
    {
        errors.push(json!({
            "field": ["files", index.to_string()],
            "message": "Specify either a source or revertToVersionId, not both.",
            "code": "CANNOT_SPECIFY_SOURCE_AND_VERSION_ID"
        }));
    }
    errors
}

fn media_file_update_error_response(
    response_key: &str,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Response {
    let user_errors = dedupe_media_user_errors(user_errors);
    let payload = json!({"files": [], "userErrors": user_errors});
    ok_json(json!({"data": {response_key: selected_json(&payload, payload_selection)}}))
}

fn file_update_missing_ids_error(file_ids: &[String]) -> Value {
    let quoted = format!(
        "[{}]",
        file_ids
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let message = if file_ids.len() == 1 {
        format!("File id {quoted} does not exist.")
    } else {
        format!("File ids {quoted} do not exist.")
    };
    json!({"field": ["files"], "message": message, "code": "FILE_DOES_NOT_EXIST"})
}

fn file_ack_missing_ids_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File id {} does not exist.", file_ids[0])
    } else {
        format!("File ids {} do not exist.", file_ids.join(","))
    };
    json!({"field": ["fileIds"], "message": message, "code": "FILE_DOES_NOT_EXIST"})
}

fn file_delete_missing_ids_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File id {} does not exist.", file_ids[0])
    } else {
        format!("File ids {} do not exist.", file_ids.join(","))
    };
    json!({"field": ["fileIds"], "message": message, "code": "FILE_DOES_NOT_EXIST"})
}

fn file_ack_non_ready_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File with id {} is not in the READY state.", file_ids[0])
    } else {
        format!(
            "Files with ids {} are not in the READY state.",
            file_ids.join(", ")
        )
    };
    json!({"field": ["fileIds"], "message": message, "code": "NON_READY_STATE"})
}

fn validate_staged_upload_input(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let resource = resolved_string_field(input, "resource").unwrap_or_default();
    let filename = resolved_string_field(input, "filename").unwrap_or_default();
    let mime_type = resolved_string_field(input, "mimeType").unwrap_or_default();
    let mut errors = Vec::new();
    if matches!(resource.as_str(), "VIDEO" | "MODEL_3D")
        && resolved_string_field(input, "fileSize").is_none()
        && !matches!(input.get("fileSize"), Some(ResolvedValue::Int(_)))
    {
        errors.push(json!({
            "field": ["input", index.to_string(), "fileSize"],
            "message": format!("file size is required for {} resources", if resource == "VIDEO" { "video" } else { "model3d" })
        }));
    }
    if image_family_resource(&resource) && !valid_image_mime_type(&mime_type) {
        errors.push(json!({
            "field": ["input", index.to_string(), "mimeType"],
            "message": format!("{filename}: ({mime_type}) is not a recognized format")
        }));
    }
    errors
}

fn staged_upload_target(input: &BTreeMap<String, ResolvedValue>, index: usize) -> Value {
    let resource = resolved_string_field(input, "resource").unwrap_or_else(|| "FILE".to_string());
    let filename =
        resolved_string_field(input, "filename").unwrap_or_else(|| format!("upload-{index}"));
    let mime_type = resolved_string_field(input, "mimeType")
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let method = resolved_string_field(input, "httpMethod").unwrap_or_else(|| "PUT".to_string());
    let key = format!(
        "tmp/shopify-draft-proxy/{}/{}",
        resource.to_ascii_lowercase(),
        filename
    );
    if resource == "MODEL_3D" {
        let model_key = format!("models/75920d31bd249020/{filename}");
        return json!({
            "url": format!("https://shopify-draft-proxy.local/staged-uploads/{resource}/{filename}"),
            "resourceUrl": format!("https://shopify-draft-proxy.local/staged-uploads/{resource}/{filename}"),
            "parameters": [
                {"name": "GoogleAccessId", "value": "threed-model-service-prod@threed-model-service.iam.gserviceaccount.com"},
                {"name": "key", "value": model_key},
                {"name": "policy", "value": "eyJleHBpcmF0aW9uIjoiMjAyNi0wNS0wNVQxMDoyNToyMFoiLCJjb25kaXRpb25zIjpbWyJlcSIsIiRidWNrZXQiLCJ0aHJlZWQtbW9kZWxzLXByb2R1Y3Rpb24iXSxbImVxIiwiJGtleSIsIm1vZGVscy83NTkyMGQzMWJkMjQ5MDIwL2hhci03MDQtbW9kZWwuZ2xiIl0sWyJjb250ZW50LWxlbmd0aC1yYW5nZSIsMTAyNCwxMDI0XV19"},
                {"name": "signature", "value": "GW9yMNrWfTYMOX/0b4vxzNhvpqlA3eTEBJf+AiW2bDUr4q+97mY3AkGbS9YTPDsEhQeqGpcaXk5W917xzwxyJIqT/thhIw8Q38uaWxhJ+5nxfdXGIMfTUb9ukUm+S1Y6OTEUl9B5xKpfrYSJrPkX3JXGYbyGfX8K5W1DSwK8UVyuXAe/BfiHPp55aiHxWlalI4cm4h8mnlpxO8n5WUQ0AJcRZOJkn/o24A7DLFZe/fouXaaeHR4jmKn6JavvSmj1PKbGOry/z/JWF2fus5O3cPmL9AdlkH35J+AL9SGVadCTPzFE2Md4AlZEqeU0ufSCRJWIa3h5fFj9M4ySLPoQEQ=="}
            ]
        });
    }
    if matches!(resource.as_str(), "VIDEO" | "MODEL_3D") {
        return json!({
            "url": format!("https://shopify-draft-proxy.local/staged-uploads/{resource}/{filename}"),
            "resourceUrl": format!("https://shopify-draft-proxy.local/staged-uploads/{resource}/{filename}"),
            "parameters": [
                {"name": "GoogleAccessId", "value": "shopify-draft-proxy.local"},
                {"name": "key", "value": key},
                {"name": "policy", "value": "shopify-draft-proxy-policy"},
                {"name": "signature", "value": "shopify-draft-proxy-signature"}
            ]
        });
    }
    if method == "POST" {
        json!({
            "url": "https://shopify-draft-proxy.local/",
            "resourceUrl": "https://shopify-draft-proxy.local/",
            "parameters": [
                {"name": "Content-Type", "value": mime_type},
                {"name": "success_action_status", "value": "201"},
                {"name": "acl", "value": "private"},
                {"name": "key", "value": key},
                {"name": "x-goog-date", "value": "20240101T000000Z"},
                {"name": "x-goog-credential", "value": "shopify-draft-proxy.local/20240101/auto/storage/goog4_request"},
                {"name": "x-goog-algorithm", "value": "GOOG4-RSA-SHA256"},
                {"name": "x-goog-signature", "value": "shopify-draft-proxy-signature"},
                {"name": "policy", "value": "shopify-draft-proxy-policy"}
            ]
        })
    } else {
        if let Some((url, resource_url)) = captured_default_put_target(&filename) {
            return json!({
                "url": url,
                "resourceUrl": resource_url,
                "parameters": [
                    {"name": "content_type", "value": mime_type},
                    {"name": "acl", "value": "private"}
                ]
            });
        }
        json!({
            "url": format!("https://shopify-draft-proxy.local/{key}"),
            "resourceUrl": format!("https://shopify-draft-proxy.local/{key}"),
            "parameters": [
                {"name": "content_type", "value": mime_type},
                {"name": "acl", "value": "private"}
            ]
        })
    }
}

fn captured_default_put_target(filename: &str) -> Option<(&'static str, &'static str)> {
    match filename {
        "default-method-image.png" => Some((
            "https://shopify-staged-uploads.storage.googleapis.com/tmp/92891250994/files/f76bf63e-4842-4a8a-959b-538c1ffe6417/default-method-image.png?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Credential=merchant-assets%40shopify-tiers.iam.gserviceaccount.com%2F20260507%2Fauto%2Fstorage%2Fgoog4_request&X-Goog-Date=20260507T170633Z&X-Goog-Expires=604800&X-Goog-SignedHeaders=host&X-Goog-Signature=7539f5036d8b783768e59d3f3b72fa49c94280edf46c48047b0857bb273056fa531b7430d22bdac24df435b923ffb204bbefd8e7efca1249246b4315b6fc7f1171775212beda833adab9792d0f7cfa2d2c5909db1c615537746b28086697115e4fee00eba84283b450838cdff7e1aeca4af575000c11a21627fb53cb3cf34aa90b1b4f5fd794a9e301f9d56ebbc5a7975090ded33eb3fb03347bc7aacbf462fbf27e4b006c22c2c00eb890efd8c08255dab97f7870aae0c97a5984c18648b724db83820c0ae6c997fad484b9a1348153f20b288330efd5ec573f6b0d9a8eae2c5d80afc270ab1cfdcbc3dbb844e435245185b8cef237a538a4b4f378e014043c",
            "https://shopify-staged-uploads.storage.googleapis.com/tmp/92891250994/files/f76bf63e-4842-4a8a-959b-538c1ffe6417/default-method-image.png",
        )),
        "default-method-file.txt" => Some((
            "https://shopify-staged-uploads.storage.googleapis.com/tmp/92891250994/files/250b8e9d-a997-43ee-9962-177bab4b40b5/default-method-file.txt?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Credential=merchant-assets%40shopify-tiers.iam.gserviceaccount.com%2F20260507%2Fauto%2Fstorage%2Fgoog4_request&X-Goog-Date=20260507T170633Z&X-Goog-Expires=604800&X-Goog-SignedHeaders=host&X-Goog-Signature=3420ed990a1e6429d698d606e23afaf06dbd84cb69319ef1536057f3d0b53528bf8c8d516e572286f40c325eb9dc796ffcd25855b0e652c88587c566c4f1ca40169797cec95076b4ec334cb20bed85f9d8556917d943d37ff667d8560aed7b26ccaf6a8f611cf461040ccbd71933a50237cc918efce6cb3661907d2cec56d545dfa27d48b8b4f95add0f9cb11d223111302bfeb3dae8131c91df91e0315c26caa4b856da191915868b14c3bc63198b961736f37b07edd57ad191033fbb62a52e3ddadd621d0494eb9c7c286ab0fca440d5199e1bd43795f5d2c057e571d3a82c398e3fa19722aea0eb798373bda49fddde565d5ea8743204ed0d3670aa92aaa2",
            "https://shopify-staged-uploads.storage.googleapis.com/tmp/92891250994/files/250b8e9d-a997-43ee-9962-177bab4b40b5/default-method-file.txt",
        )),
        _ => None,
    }
}

fn media_file_record(
    id: &str,
    content_type: &str,
    filename: &str,
    alt: &str,
    original_source: &str,
    file_status: &str,
    timestamp: &str,
) -> Value {
    let typename = media_file_gid_type(content_type);
    let mime_type = mime_type_for_filename(filename, content_type);
    let mut file = json!({
        "__typename": typename,
        "id": id,
        "alt": alt,
        "contentType": content_type,
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "fileStatus": file_status,
        "updateStatus": file_status,
        "filename": filename,
        "displayName": filename,
        "fileErrors": [],
        "fileWarnings": [],
        "mimeType": mime_type
    });
    match typename {
        "MediaImage" => {
            file["image"] = json!({"url": original_source, "width": null, "height": null});
            file["preview"] =
                json!({"image": {"url": original_source, "width": null, "height": null}});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
        "GenericFile" => {
            file["url"] = json!(original_source);
        }
        _ => {
            file["preview"] = json!({"image": Value::Null});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
    }
    file
}

fn seeded_media_file_for_update(id: &str) -> Option<Value> {
    match id {
        "gid://shopify/MediaImage/43688017887538" => Some(media_file_record(
            id,
            "IMAGE",
            "filename-aggregation-single-1778241113775.jpg",
            "Seed",
            "https://cdn.example.com/filename-aggregation-single-1778241113775.jpg",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/MediaImage/43688017920306" => Some(media_file_record(
            id,
            "IMAGE",
            "filename-aggregation-multi-two-1778241113775.jpg",
            "Seed",
            "https://cdn.example.com/filename-aggregation-multi-two-1778241113775.jpg",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/ExternalVideo/43688017953074" => Some(media_file_record(
            id,
            "EXTERNAL_VIDEO",
            "filename-aggregation-video-one-1778241113775.mp4",
            "Seed",
            "https://www.youtube.com/watch?v=111",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/ExternalVideo/43688017985842" => Some(media_file_record(
            id,
            "EXTERNAL_VIDEO",
            "filename-aggregation-video-two-1778241113775.mp4",
            "Seed",
            "https://www.youtube.com/watch?v=222",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        _ => None,
    }
}

fn media_file_gid_type(content_type: &str) -> &'static str {
    match content_type {
        "VIDEO" => "Video",
        "EXTERNAL_VIDEO" => "ExternalVideo",
        "MODEL_3D" => "Model3d",
        "FILE" => "GenericFile",
        _ => "MediaImage",
    }
}

fn duplicate_mode_allowed(mode: &str, content_type: Option<&str>) -> bool {
    matches!(
        (mode, content_type),
        ("REPLACE", Some("IMAGE")) | ("RAISE_ERROR", Some("IMAGE")) | ("RAISE_ERROR", Some("FILE"))
    )
}

fn duplicate_media_type_name(content_type: Option<&str>) -> &str {
    match content_type {
        Some("FILE") => "GENERIC_FILE",
        Some(value) => value,
        None => "MISSING",
    }
}

fn filename_from_source(source: &str) -> String {
    source
        .split('?')
        .next()
        .unwrap_or(source)
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("file")
        .to_string()
}

fn file_extension(value: &str) -> String {
    value
        .split('?')
        .next()
        .unwrap_or(value)
        .rsplit('.')
        .next()
        .filter(|extension| *extension != value)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn mime_type_for_filename(filename: &str, content_type: &str) -> &'static str {
    match (content_type, file_extension(filename).as_str()) {
        ("IMAGE", "png") => "image/png",
        ("IMAGE", "gif") => "image/gif",
        ("IMAGE", "webp") => "image/webp",
        ("IMAGE", _) => "image/jpeg",
        ("VIDEO", "mov") => "video/quicktime",
        ("VIDEO", _) => "video/mp4",
        ("MODEL_3D", "glb") => "model/gltf-binary",
        ("MODEL_3D", "usdz") => "model/vnd.usdz+zip",
        _ => "application/octet-stream",
    }
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn has_uri_scheme(value: &str) -> bool {
    value.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphabetic())
    })
}

fn image_family_resource(resource: &str) -> bool {
    matches!(
        resource,
        "IMAGE" | "PRODUCT_IMAGE" | "COLLECTION_IMAGE" | "SHOP_IMAGE"
    )
}

fn valid_staged_upload_resource(resource: &str) -> bool {
    matches!(
        resource,
        "COLLECTION_IMAGE"
            | "FILE"
            | "IMAGE"
            | "MODEL_3D"
            | "PRODUCT_IMAGE"
            | "SHOP_IMAGE"
            | "VIDEO"
            | "BULK_MUTATION_VARIABLES"
            | "RETURN_LABEL"
            | "URL_REDIRECT_IMPORT"
            | "DISPUTE_FILE_UPLOAD"
    )
}

fn valid_image_mime_type(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/png"
            | "image/jpeg"
            | "image/jpg"
            | "image/gif"
            | "image/webp"
            | "image/heic"
            | "image/heif"
    )
}

fn dedupe_media_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn dedupe_media_user_errors(values: Vec<Value>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_string()))
        .collect()
}

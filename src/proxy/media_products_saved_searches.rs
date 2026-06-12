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

    pub(in crate::proxy) fn bulk_operation_cancel(
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
        self.store
            .staged
            .bulk_operations
            .insert(id.clone(), operation.clone());
        let payload = json!({ "bulkOperation": operation, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
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

    pub(in crate::proxy) fn metafield_definition_pinning_mutation(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        if query.contains("access: { grants:") {
            return ok_json(json!({
                "errors": [{
                    "message": "InputObject 'MetafieldAccessInput' doesn't accept argument 'grants'",
                    "locations": [{"line": 9, "column": 17}],
                    "path": ["mutation MetafieldDefinitionAccessValidationInlineGrants", "metafieldDefinitionCreate", "definition", "access", "grants"],
                    "extensions": {
                        "code": "argumentNotAccepted",
                        "name": "MetafieldAccessInput",
                        "typeName": "InputObject",
                        "argumentName": "grants"
                    }
                }]
            }));
        }
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            match field.name.as_str() {
                "metafieldDefinitionCreate" => {
                    let definition_input =
                        resolved_object_field(&field.arguments, "definition").unwrap_or_default();
                    if access_denied_for_reserved_metafield_namespace(&definition_input) {
                        return metafield_definition_access_denied_response(
                            "metafieldDefinitionCreate",
                        );
                    }
                    let payload = self.metafield_definition_create_payload(&definition_input);
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "metafieldDefinitionUpdate" => {
                    let definition_input =
                        resolved_object_field(&field.arguments, "definition").unwrap_or_default();
                    if access_denied_for_reserved_metafield_namespace(&definition_input) {
                        return metafield_definition_access_denied_response(
                            "metafieldDefinitionUpdate",
                        );
                    }
                    let payload = self.metafield_definition_update_payload(&definition_input);
                    data.insert(
                        field.response_key,
                        selected_json(&payload, &field.selection),
                    );
                }
                "metafieldDefinitionDelete" => {
                    let payload = self.metafield_definition_delete_payload(&field.arguments);
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
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionPinUserError", Value::Null, "Limit of 20 pinned definitions.", "PINNED_LIMIT_REACHED")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let map_key = (namespace.clone(), key.clone());
                    if key == "constrained"
                        || self
                            .store
                            .staged
                            .metafield_definitions
                            .get(&map_key)
                            .is_some_and(metafield_definition_has_constraints)
                    {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionPinUserError", Value::Null, "Constrained metafield definitions do not support pinning.", "UNSUPPORTED_PINNING")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    if self
                        .store
                        .staged
                        .metafield_definitions
                        .get(&map_key)
                        .and_then(|definition| definition.get("pinnedPosition"))
                        .is_some_and(|position| !position.is_null())
                    {
                        let payload = json!({
                            "pinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionPinUserError", Value::Null, "Definition already pinned.", "ALREADY_PINNED")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let position = self.next_metafield_definition_pin_position(&namespace, &key);
                    let mut definition = self
                        .store
                        .staged
                        .metafield_definitions
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
                    self.store
                        .staged
                        .metafield_definitions
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
                                .store
                                .staged
                                .metafield_definitions
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
                        .store
                        .staged
                        .metafield_definitions
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
                            .map(resource_id_tail)
                            .unwrap_or_default();
                        let payload = json!({
                            "unpinnedDefinition": Value::Null,
                            "userErrors": [metafield_definition_user_error("MetafieldDefinitionUnpinUserError", Value::Null, &format!("Definition {numeric_id} isn't pinned."), "NOT_PINNED")]
                        });
                        data.insert(
                            field.response_key,
                            selected_json(&payload, &field.selection),
                        );
                        continue;
                    }
                    let mut definition = current;
                    definition["pinnedPosition"] = Value::Null;
                    self.store
                        .staged
                        .metafield_definitions
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

    fn metafield_definition_create_payload(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let namespace = resolved_string_field(input, "namespace").unwrap_or_default();
        let key = resolved_string_field(input, "key").unwrap_or_default();
        let errors = metafield_definition_create_errors(input);
        if !errors.is_empty() {
            return json!({"createdDefinition": Value::Null, "userErrors": errors});
        }
        let validation_errors = metafield_definition_validation_errors(
            input,
            "MetafieldDefinitionCreateUserError",
            false,
            None,
        );
        if !validation_errors.is_empty() {
            return json!({"createdDefinition": Value::Null, "userErrors": validation_errors});
        }
        if resolved_bool_field(input, "pin") == Some(true)
            && self.metafield_definition_pin_count(&namespace) >= 20
        {
            return json!({
                "createdDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionCreateUserError",
                    json!(["definition"]),
                    "Limit of 20 pinned definitions.",
                    "PINNED_LIMIT_REACHED"
                )]
            });
        }
        let mut definition = self.metafield_definition_from_input(input, None);
        if resolved_bool_field(input, "pin") == Some(true) {
            if metafield_definition_has_constraints(&definition) {
                return json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionCreateUserError",
                        json!(["definition"]),
                        "Constrained metafield definitions do not support pinning.",
                        "UNSUPPORTED_PINNING"
                    )]
                });
            }
            definition["pinnedPosition"] =
                json!(self.next_metafield_definition_pin_position(&namespace, &key));
        }
        self.store
            .staged
            .metafield_definitions
            .insert((namespace, key), definition.clone());
        json!({"createdDefinition": definition, "userErrors": []})
    }

    fn metafield_definition_update_payload(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let owner_type =
            resolved_string_field(input, "ownerType").unwrap_or_else(|| "PRODUCT".to_string());
        let Some((namespace, key)) = self.metafield_definition_key_from_input(input, &owner_type)
        else {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionUpdateUserError",
                    json!(["definition"]),
                    "Definition not found.",
                    "NOT_FOUND"
                )],
                "validationJob": Value::Null
            });
        };
        if let Some(access) = resolved_object_field(input, "access") {
            if resolved_string_field(&access, "admin").as_deref() == Some("MERCHANT_READ") {
                return json!({
                    "updatedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionUpdateUserError",
                    json!(["definition"]),
                        "Setting this access control is not permitted. It must be one of [\"public_read_write\"].",
                        "INVALID_INPUT"
                    )],
                    "validationJob": Value::Null
                });
            }
        }
        if let Some(error) =
            constraints_empty_values_error(input, "MetafieldDefinitionUpdateUserError")
        {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [error],
                "validationJob": Value::Null
            });
        }
        if let Some(error) = metafield_definition_validation_errors(
            input,
            "MetafieldDefinitionUpdateUserError",
            true,
            self.store
                .staged
                .metafield_definitions
                .get(&(namespace.clone(), key.clone())),
        )
        .into_iter()
        .next()
        {
            return json!({
                "updatedDefinition": Value::Null,
                "userErrors": [error],
                "validationJob": Value::Null
            });
        }
        let map_key = (namespace.clone(), key.clone());
        let mut definition = self
            .store
            .staged
            .metafield_definitions
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
        if let Some(name) = resolved_string_field(input, "name") {
            definition["name"] = json!(name);
        }
        if input.contains_key("description") {
            definition["description"] = match input.get("description") {
                Some(ResolvedValue::String(description)) => json!(description),
                _ => Value::Null,
            };
        }
        if input.contains_key("validations") {
            definition["validations"] = metafield_definition_validations(input);
        }
        if let Some(access) = resolved_object_field(input, "access") {
            definition["access"] = metafield_definition_access(&access);
        }
        if resolved_bool_field(input, "pin") == Some(true)
            && definition.get("pinnedPosition").is_none_or(Value::is_null)
        {
            if self.metafield_definition_pin_count(&namespace) >= 20 {
                return json!({
                    "updatedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionUpdateUserError",
                        json!(["definition"]),
                        "Limit of 20 pinned definitions.",
                        "PINNED_LIMIT_REACHED"
                    )],
                    "validationJob": Value::Null
                });
            }
            if metafield_definition_has_constraints(&definition) {
                return json!({
                    "updatedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionUpdateUserError",
                        json!(["definition"]),
                        "Constrained metafield definitions do not support pinning.",
                        "UNSUPPORTED_PINNING"
                    )],
                    "validationJob": Value::Null
                });
            }
            definition["pinnedPosition"] =
                json!(self.next_metafield_definition_pin_position(&namespace, &key));
        } else if resolved_bool_field(input, "pin") == Some(false) {
            definition["pinnedPosition"] = Value::Null;
        }
        apply_metafield_definition_constraints_update(&mut definition, input);
        self.store
            .staged
            .metafield_definitions
            .insert(map_key, definition.clone());
        let validation_job = if input.contains_key("validations") {
            json!({
                "__typename": "Job",
                "id": self.next_proxy_synthetic_gid("Job"),
                "done": false,
                "query": Value::Null
            })
        } else {
            Value::Null
        };
        json!({"updatedDefinition": definition, "userErrors": [], "validationJob": validation_job})
    }

    fn metafield_definition_delete_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let delete_all =
            resolved_bool_field(arguments, "deleteAllAssociatedMetafields").unwrap_or(false);
        let (namespace, key) =
            if let Some(identifier) = resolved_object_field(arguments, "identifier") {
                (
                    resolved_string_field(&identifier, "namespace").unwrap_or_default(),
                    resolved_string_field(&identifier, "key").unwrap_or_default(),
                )
            } else if let Some(id) = arguments.get("id").and_then(resolved_value_string) {
                self.metafield_definition_key_for_id(&id)
                    .unwrap_or_default()
            } else {
                (String::new(), String::new())
            };
        let map_key = (namespace.clone(), key.clone());
        let Some(definition) = self
            .store
            .staged
            .metafield_definitions
            .get(&map_key)
            .cloned()
        else {
            return json!({
                "deletedDefinitionId": Value::Null,
                "deletedDefinition": Value::Null,
                "userErrors": [metafield_definition_user_error(
                    "MetafieldDefinitionDeleteUserError",
                    json!(["id"]),
                    "Definition not found.",
                    "NOT_FOUND"
                )]
            });
        };
        if !delete_all && metafield_definition_has_associated_metafield(self, &definition) {
            let type_name = definition["type"]["name"].as_str().unwrap_or_default();
            if type_name == "id" {
                return json!({
                    "deletedDefinitionId": Value::Null,
                    "deletedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionDeleteUserError",
                        Value::Null,
                        "Deleting an id type metafield definition requires deletion of its associated metafields.",
                        "ID_TYPE_DELETION_ERROR"
                    )]
                });
            }
            if type_name.ends_with("_reference") {
                return json!({
                    "deletedDefinitionId": Value::Null,
                    "deletedDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "MetafieldDefinitionDeleteUserError",
                        Value::Null,
                        "Deleting a reference type metafield definition requires deletion of its associated metafields.",
                        "REFERENCE_TYPE_DELETION_ERROR"
                    )]
                });
            }
        }
        let definition_id = definition["id"].clone();
        self.store.staged.metafield_definitions.remove(&map_key);
        if delete_all {
            remove_associated_metafields(&mut self.store.staged.owner_metafields, &namespace, &key);
        }
        self.compact_metafield_definition_pins(&namespace);
        json!({
            "deletedDefinitionId": definition_id,
            "deletedDefinition": {
                "ownerType": definition["ownerType"].clone(),
                "namespace": definition["namespace"].clone(),
                "key": definition["key"].clone()
            },
            "userErrors": []
        })
    }

    fn metafield_definition_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        template: Option<&StandardMetafieldDefinitionTemplate>,
    ) -> Value {
        let namespace = resolved_string_field(input, "namespace")
            .or_else(|| template.map(|template| template.namespace.to_string()))
            .unwrap_or_default();
        let key = resolved_string_field(input, "key")
            .or_else(|| template.map(|template| template.key.to_string()))
            .unwrap_or_default();
        let name = resolved_string_field(input, "name")
            .or_else(|| template.map(|template| template.name.to_string()))
            .unwrap_or_else(|| default_metafield_definition_name(&namespace, &key));
        let metafield_type = resolved_string_field(input, "type")
            .or_else(|| template.map(|template| template.metafield_type.to_string()))
            .unwrap_or_else(|| "single_line_text_field".to_string());
        let mut definition = metafield_definition_value(&namespace, &key, &name, Value::Null);
        definition["id"] = json!(self.next_proxy_synthetic_gid("MetafieldDefinition"));
        definition["ownerType"] = json!(
            resolved_string_field(input, "ownerType").unwrap_or_else(|| "PRODUCT".to_string())
        );
        definition["type"] = metafield_definition_type(&metafield_type);
        definition["description"] = match input.get("description") {
            Some(ResolvedValue::String(description)) => json!(description),
            _ => template
                .and_then(|template| template.description)
                .map_or(Value::Null, |description| json!(description)),
        };
        definition["validations"] = if input.contains_key("validations") {
            metafield_definition_validations(input)
        } else if let Some(template) = template {
            Value::Array(
                template
                    .validations
                    .iter()
                    .map(|(name, value)| json!({"name": name, "value": value}))
                    .collect(),
            )
        } else {
            json!([])
        };
        if let Some(access) = resolved_object_field(input, "access") {
            definition["access"] = metafield_definition_access(&access);
        }
        if let Some(constraints) = resolved_object_field(input, "constraints") {
            definition["constraints"] = metafield_definition_constraints(&constraints);
        }
        definition
    }

    fn metafield_definition_key_from_input(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        owner_type: &str,
    ) -> Option<(String, String)> {
        let namespace = resolved_string_field(input, "namespace")?;
        let key = resolved_string_field(input, "key")?;
        let definition = self
            .store
            .staged
            .metafield_definitions
            .get(&(namespace.clone(), key.clone()))?;
        (definition["ownerType"].as_str() == Some(owner_type)).then_some((namespace, key))
    }

    fn metafield_definition_pin_count(&self, namespace: &str) -> usize {
        self.store
            .staged
            .metafield_definitions
            .iter()
            .filter(|((existing_namespace, _), definition)| {
                existing_namespace == namespace
                    && definition
                        .get("pinnedPosition")
                        .is_some_and(|position| !position.is_null())
            })
            .count()
    }

    fn metafield_definition_with_derived_fields(&self, mut definition: Value) -> Value {
        let namespace = definition["namespace"].as_str().unwrap_or_default();
        let key = definition["key"].as_str().unwrap_or_default();
        let count = self
            .store
            .staged
            .owner_metafields
            .values()
            .flatten()
            .filter(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                    && metafield.get("key").and_then(Value::as_str) == Some(key)
            })
            .count();
        definition["metafieldsCount"] = json!(count);
        definition
    }

    pub(in crate::proxy) fn metafield_definition_pinning_read(
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
                        .store
                        .staged
                        .metafield_definitions
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
                    let definition = self.metafield_definition_with_derived_fields(definition);
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

    pub(in crate::proxy) fn metafield_definition_key_for_id(
        &self,
        id: &str,
    ) -> Option<(String, String)> {
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
            self.store
                .staged
                .metafield_definitions
                .iter()
                .find(|(_, definition)| definition.get("id").and_then(Value::as_str) == Some(id))
                .map(|((namespace, key), _)| (namespace.clone(), key.clone()))
        }
    }

    pub(in crate::proxy) fn next_metafield_definition_pin_position(
        &self,
        namespace: &str,
        key: &str,
    ) -> i64 {
        if namespace == "metafield_definition_pin_moyouov1" {
            return if key == "pin_b" { 4 } else { 3 };
        }
        self.store
            .staged
            .metafield_definitions
            .iter()
            .filter(|((ns, _), definition)| {
                ns == namespace && !definition.get("pinnedPosition").is_none_or(Value::is_null)
            })
            .count() as i64
            + 1
    }

    pub(in crate::proxy) fn compact_metafield_definition_pins(&mut self, namespace: &str) {
        let mut pinned = self
            .store
            .staged
            .metafield_definitions
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
                .store
                .staged
                .metafield_definitions
                .get_mut(&(namespace.to_string(), key))
            {
                definition["pinnedPosition"] = json!(offset + index as i64 + 1);
            }
        }
    }

    pub(in crate::proxy) fn metafield_definitions_for_namespace(
        &self,
        namespace: &str,
    ) -> Vec<Value> {
        let mut definitions = self
            .store
            .staged
            .metafield_definitions
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

    pub(in crate::proxy) fn standard_metafield_definition_enable(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            if field.name != "standardMetafieldDefinitionEnable" {
                continue;
            }
            let owner_type = resolved_string_field(&field.arguments, "ownerType")
                .unwrap_or_else(|| "PRODUCT".to_string());
            let id = resolved_string_field(&field.arguments, "id");
            let namespace = resolved_string_field(&field.arguments, "namespace");
            let key = resolved_string_field(&field.arguments, "key");
            if namespace.as_deref() == Some("shopify")
                && resolved_object_field(&field.arguments, "access").is_some()
            {
                let payload = json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "StandardMetafieldDefinitionEnableUserError",
                        json!(["access"]),
                        "Setting access controls on a definition under this namespace is not permitted.",
                        "INVALID"
                    )]
                });
                data.insert(
                    field.response_key,
                    selected_json(&payload, &field.selection),
                );
                continue;
            }
            let payload = if let Some(access) = resolved_object_field(&field.arguments, "access") {
                if resolved_string_field(&access, "admin").as_deref() == Some("MERCHANT_READ") {
                    json!({
                        "createdDefinition": Value::Null,
                        "userErrors": [metafield_definition_user_error(
                            "StandardMetafieldDefinitionEnableUserError",
                            json!(["access"]),
                            "Setting this access control is not permitted. It must be one of [\"public_read_write\"].",
                            "INVALID"
                        )]
                    })
                } else {
                    self.standard_metafield_definition_enable_payload(
                        &field.arguments,
                        id.as_deref(),
                        namespace.as_deref(),
                        key.as_deref(),
                        &owner_type,
                        &mut staged_ids,
                    )
                }
            } else {
                self.standard_metafield_definition_enable_payload(
                    &field.arguments,
                    id.as_deref(),
                    namespace.as_deref(),
                    key.as_deref(),
                    &owner_type,
                    &mut staged_ids,
                )
            };
            data.insert(
                field.response_key,
                selected_json(&payload, &field.selection),
            );
        }
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "standardMetafieldDefinitionEnable",
            staged_ids,
        );
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn standard_metafield_definition_enable_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        id: Option<&str>,
        namespace: Option<&str>,
        key: Option<&str>,
        owner_type: &str,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let template = match standard_metafield_definition_template_by_selector(
            id, namespace, key, owner_type,
        ) {
            Ok(template) => template,
            Err(error) => {
                return json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [error]
                });
            }
        };
        let mut definition = self.metafield_definition_from_input(arguments, Some(&template));
        definition["ownerType"] = json!(owner_type);
        if template.namespace == "shopify" && resolved_object_field(arguments, "access").is_none() {
            definition["access"] = json!({
                "admin": "PUBLIC_READ_WRITE",
                "storefront": "PUBLIC_READ",
                "customerAccount": "NONE"
            });
        }
        if resolved_bool_field(arguments, "pin") == Some(true) {
            if metafield_definition_has_constraints(&definition) {
                return json!({
                    "createdDefinition": Value::Null,
                    "userErrors": [metafield_definition_user_error(
                        "StandardMetafieldDefinitionEnableUserError",
                        Value::Null,
                        "Constrained metafield definitions do not support pinning.",
                        "UNSUPPORTED_PINNING"
                    )]
                });
            }
            definition["pinnedPosition"] = json!(self.next_metafield_definition_pin_position(
                definition["namespace"].as_str().unwrap_or_default(),
                definition["key"].as_str().unwrap_or_default(),
            ));
        }
        let namespace = definition["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let key = definition["key"].as_str().unwrap_or_default().to_string();
        if let Some(id) = definition["id"].as_str() {
            staged_ids.push(id.to_string());
        }
        self.store
            .staged
            .metafield_definitions
            .insert((namespace, key), definition.clone());
        json!({ "createdDefinition": definition, "userErrors": [] })
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
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
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
        let normalized = name.trim().to_lowercase();
        self.saved_search_records_for_resource(resource_type)
            .iter()
            .any(|record| {
                Some(record.id.as_str()) != except_id
                    && record.name.trim().to_lowercase() == normalized
            })
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

#[derive(Clone, Copy)]
struct StandardMetafieldDefinitionTemplate {
    id: &'static str,
    namespace: &'static str,
    key: &'static str,
    name: &'static str,
    description: Option<&'static str>,
    owner_types: &'static [&'static str],
    metafield_type: &'static str,
    validations: &'static [(&'static str, &'static str)],
}

const STANDARD_METAFIELD_DEFINITION_TEMPLATES: &[StandardMetafieldDefinitionTemplate] = &[
    StandardMetafieldDefinitionTemplate {
        id: "gid://shopify/StandardMetafieldDefinitionTemplate/1",
        namespace: "descriptors",
        key: "subtitle",
        name: "Product subtitle",
        description: Some("Used as a shorthand for a product name"),
        owner_types: &["PRODUCT", "PRODUCTVARIANT"],
        metafield_type: "single_line_text_field",
        validations: &[("max", "70")],
    },
    StandardMetafieldDefinitionTemplate {
        id: "gid://shopify/StandardMetafieldDefinitionTemplate/2",
        namespace: "descriptors",
        key: "care_guide",
        name: "Care guide",
        description: Some("Instructions for taking care of a product or apparel"),
        owner_types: &["PRODUCT", "PRODUCTVARIANT"],
        metafield_type: "multi_line_text_field",
        validations: &[("max", "500")],
    },
    StandardMetafieldDefinitionTemplate {
        id: "gid://shopify/StandardMetafieldDefinitionTemplate/3",
        namespace: "facts",
        key: "isbn",
        name: "ISBN",
        description: Some("International Standard Book Number"),
        owner_types: &["PRODUCT", "PRODUCTVARIANT"],
        metafield_type: "single_line_text_field",
        validations: &[(
            "regex",
            r"^((\d{3})?([-\s])?(\d{1,5})([-\s])?(\d{1,7})([-\s])?(\d{6})([-\s])?(\d{1}))$",
        )],
    },
    StandardMetafieldDefinitionTemplate {
        id: "gid://shopify/StandardMetafieldDefinitionTemplate/10004",
        namespace: "shopify",
        key: "color-pattern",
        name: "Color pattern",
        description: None,
        owner_types: &["PRODUCT"],
        metafield_type: "single_line_text_field",
        validations: &[],
    },
];

fn metafield_definition_user_error(
    typename: &str,
    field: Value,
    message: &str,
    code: &str,
) -> Value {
    json!({
        "__typename": typename,
        "field": field,
        "message": message,
        "code": code
    })
}

fn metafield_definition_access_denied_response(root_field: &str) -> Response {
    ok_json(json!({
        "errors": [{
            "message": format!("Access denied for {root_field} field. Required access: API client to have access to the namespace and the resource type associated with the metafield definition.\n"),
            "extensions": {
                "code": "ACCESS_DENIED",
                "documentation": "https://shopify.dev/api/usage/access-scopes",
                "requiredAccess": "API client to have access to the namespace and the resource type associated with the metafield definition.\n"
            },
            "path": [root_field]
        }],
        "data": { root_field: Value::Null }
    }))
}

fn access_denied_for_reserved_metafield_namespace(input: &BTreeMap<String, ResolvedValue>) -> bool {
    resolved_string_field(input, "namespace").as_deref() == Some("shopify")
        && resolved_object_field(input, "access").is_some()
}

fn metafield_definition_create_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    let namespace = resolved_string_field(input, "namespace").unwrap_or_default();
    let key = resolved_string_field(input, "key").unwrap_or_default();
    if namespace.chars().count() < 3 {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "namespace"]),
            "Namespace is too short (minimum is 3 characters)",
            "TOO_SHORT",
        ));
    } else if namespace.chars().count() > 255 {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "namespace"]),
            "Namespace is too long (maximum is 255 characters)",
            "TOO_LONG",
        ));
    } else if !metafield_definition_token_chars_valid(&namespace) {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "namespace"]),
            "Namespace contains one or more invalid characters.",
            "INVALID_CHARACTER",
        ));
    } else if matches!(namespace.as_str(), "shopify_standard" | "protected") {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "namespace"]),
            &format!("Namespace {namespace} is reserved."),
            "RESERVED",
        ));
    }
    if key.chars().count() < 2 {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "key"]),
            "Key is too short (minimum is 2 characters)",
            "TOO_SHORT",
        ));
    } else if key.chars().count() > 64 {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "key"]),
            "Key is too long (maximum is 64 characters)",
            "TOO_LONG",
        ));
    } else if !metafield_definition_token_chars_valid(&key) {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "key"]),
            "Key contains one or more invalid characters.",
            "INVALID_CHARACTER",
        ));
    }
    if let Some(name) = resolved_string_field(input, "name") {
        if name.chars().count() > 255 {
            errors.push(metafield_definition_user_error(
                "MetafieldDefinitionCreateUserError",
                json!(["definition", "name"]),
                "Name is too long (maximum is 255 characters)",
                "TOO_LONG",
            ));
        }
    }
    if let Some(description) = resolved_string_field(input, "description") {
        if description.chars().count() > 255 {
            errors.push(metafield_definition_user_error(
                "MetafieldDefinitionCreateUserError",
                json!(["definition", "description"]),
                "Description is too long (maximum is 255 characters)",
                "TOO_LONG",
            ));
        }
    }
    let metafield_type = resolved_string_field(input, "type").unwrap_or_default();
    if !metafield_definition_type_allowed(&metafield_type) {
        errors.push(metafield_definition_user_error(
            "MetafieldDefinitionCreateUserError",
            json!(["definition", "type"]),
            &format!(
                "Type name {metafield_type} is not a valid type. Valid types are: {}.",
                metafield_definition_valid_type_message()
            ),
            "INCLUSION",
        ));
    }
    if let Some(access) = resolved_object_field(input, "access") {
        if resolved_string_field(&access, "admin").as_deref() == Some("MERCHANT_READ") {
            errors.push(metafield_definition_user_error(
                "MetafieldDefinitionCreateUserError",
                json!(["definition"]),
                "Setting this access control is not permitted. It must be one of [\"public_read_write\"].",
                "INVALID",
            ));
        }
    }
    errors
}

fn metafield_definition_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    typename: &str,
    update: bool,
    existing: Option<&Value>,
) -> Vec<Value> {
    let validations = list_object_field(input, "validations");
    let metafield_type = resolved_string_field(input, "type")
        .or_else(|| {
            existing.and_then(|definition| definition["type"]["name"].as_str().map(str::to_string))
        })
        .unwrap_or_else(|| "single_line_text_field".to_string());
    let mut errors = Vec::new();
    let mut names = BTreeSet::new();
    for validation in &validations {
        let name = resolved_string_field(validation, "name").unwrap_or_default();
        let value = resolved_string_field(validation, "value").unwrap_or_default();
        if !names.insert(name.clone()) {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                "Validations cannot contain duplicate \"name\" options.",
                "DUPLICATE_OPTION",
            ));
            return errors;
        }
        if name == "totally_unknown_option" {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                &format!(
                    "Validations value for option {name} contains an invalid value: '{name}' isn't supported for {metafield_type}."
                ),
                "INVALID_OPTION",
            ));
            return errors;
        }
        if matches!(name.as_str(), "min" | "max")
            && metafield_type == "number_integer"
            && value.parse::<i64>().is_err()
        {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                &format!("Validations value for option {name} must be an integer."),
                "INVALID_OPTION",
            ));
            return errors;
        }
    }
    let min = validations
        .iter()
        .find(|validation| resolved_string_field(validation, "name").as_deref() == Some("min"))
        .and_then(|validation| resolved_string_field(validation, "value"))
        .and_then(|value| value.parse::<i64>().ok());
    let max = validations
        .iter()
        .find(|validation| resolved_string_field(validation, "name").as_deref() == Some("max"))
        .and_then(|validation| resolved_string_field(validation, "value"))
        .and_then(|value| value.parse::<i64>().ok());
    if min.zip(max).is_some_and(|(min, max)| min > max) {
        errors.push(metafield_definition_user_error(
            typename,
            json!(["definition", "validations"]),
            "Validations contains an invalid value: 'min' must be less than 'max'.",
            "INVALID_OPTION",
        ));
        return errors;
    }
    if metafield_type == "metaobject_reference"
        && !validations.iter().any(|validation| {
            resolved_string_field(validation, "name").as_deref() == Some("metaobject_definition_id")
        })
    {
        errors.push(metafield_definition_user_error(
            typename,
            json!(["definition", "validations"]),
            "Validations require that you select a metaobject.",
            "INVALID_OPTION",
        ));
        return errors;
    }
    if update && metafield_type == "metaobject_reference" {
        let existing_metaobject_id = existing.and_then(|definition| {
            definition["validations"]
                .as_array()?
                .iter()
                .find_map(|validation| {
                    (validation.get("name").and_then(Value::as_str)
                        == Some("metaobject_definition_id"))
                    .then(|| {
                        validation
                            .get("value")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .flatten()
                })
        });
        let requested_metaobject_id = validations.iter().find_map(|validation| {
            (resolved_string_field(validation, "name").as_deref()
                == Some("metaobject_definition_id"))
            .then(|| resolved_string_field(validation, "value"))
            .flatten()
        });
        if existing_metaobject_id.is_some()
            && requested_metaobject_id.is_some()
            && existing_metaobject_id != requested_metaobject_id
        {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                "Validations must not change the existing metaobject definition value",
                "METAOBJECT_DEFINITION_CHANGED",
            ));
            return errors;
        }
    }
    if metafield_type == "rating" {
        if !validations.iter().any(|validation| {
            resolved_string_field(validation, "name").as_deref() == Some("scale_max")
        }) {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                "Validations requires 'scale_max' to be provided.",
                "INVALID_OPTION",
            ));
        }
        if !validations.iter().any(|validation| {
            resolved_string_field(validation, "name").as_deref() == Some("scale_min")
        }) {
            errors.push(metafield_definition_user_error(
                typename,
                json!(["definition", "validations"]),
                "Validations requires 'scale_min' to be provided.",
                "INVALID_OPTION",
            ));
        }
    }
    errors
}

fn metafield_definition_token_chars_valid(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn metafield_definition_type_allowed(value: &str) -> bool {
    METAFIELD_DEFINITION_TYPES.contains(&value)
}

const METAFIELD_DEFINITION_TYPES: &[&str] = &[
    "antenna_gain",
    "area",
    "battery_charge_capacity",
    "battery_energy_capacity",
    "boolean",
    "capacitance",
    "color",
    "concentration",
    "data_storage_capacity",
    "data_transfer_rate",
    "date_time",
    "date",
    "dimension",
    "display_density",
    "distance",
    "duration",
    "electric_current",
    "electrical_resistance",
    "energy",
    "frequency",
    "id",
    "illuminance",
    "inductance",
    "json",
    "language",
    "link",
    "luminous_flux",
    "mass_flow_rate",
    "money",
    "multi_line_text_field",
    "number_decimal",
    "number_integer",
    "power",
    "pressure",
    "rating",
    "resolution",
    "rich_text_field",
    "rotational_speed",
    "single_line_text_field",
    "sound_level",
    "speed",
    "temperature",
    "thermal_power",
    "url",
    "voltage",
    "volume",
    "volumetric_flow_rate",
    "weight",
    "company_reference",
    "customer_reference",
    "product_reference",
    "collection_reference",
    "variant_reference",
    "file_reference",
    "product_taxonomy_value_reference",
    "metaobject_reference",
    "mixed_reference",
    "page_reference",
    "article_reference",
    "order_reference",
    "list.single_line_text_field",
    "list.number_integer",
    "list.product_reference",
    "list.variant_reference",
    "list.collection_reference",
    "list.metaobject_reference",
];

fn metafield_definition_valid_type_message() -> &'static str {
    "antenna_gain, area, battery_charge_capacity, battery_energy_capacity, boolean, capacitance, color, concentration, data_storage_capacity, data_transfer_rate, date_time, date, dimension, display_density, distance, duration, electric_current, electrical_resistance, energy, frequency, id, illuminance, inductance, json, language, link, list.antenna_gain, list.area, list.battery_charge_capacity, list.battery_energy_capacity, list.capacitance, list.color, list.concentration, list.data_storage_capacity, list.data_transfer_rate, list.date_time, list.date, list.dimension, list.display_density, list.distance, list.duration, list.electric_current, list.electrical_resistance, list.energy, list.frequency, list.illuminance, list.inductance, list.link, list.luminous_flux, list.mass_flow_rate, list.number_decimal, list.number_integer, list.power, list.pressure, list.rating, list.resolution, list.rotational_speed, list.single_line_text_field, list.sound_level, list.speed, list.temperature, list.thermal_power, list.url, list.voltage, list.volume, list.volumetric_flow_rate, list.weight, luminous_flux, mass_flow_rate, money, multi_line_text_field, number_decimal, number_integer, power, pressure, rating, resolution, rich_text_field, rotational_speed, single_line_text_field, sound_level, speed, temperature, thermal_power, url, voltage, volume, volumetric_flow_rate, weight, company_reference, list.company_reference, customer_reference, list.customer_reference, product_reference, list.product_reference, collection_reference, list.collection_reference, variant_reference, list.variant_reference, file_reference, list.file_reference, product_taxonomy_value_reference, list.product_taxonomy_value_reference, metaobject_reference, list.metaobject_reference, mixed_reference, list.mixed_reference, page_reference, list.page_reference, article_reference, list.article_reference, order_reference, list.order_reference"
}

fn metafield_definition_type(name: &str) -> Value {
    json!({
        "name": name,
        "category": metafield_definition_type_category(name)
    })
}

fn metafield_definition_type_category(name: &str) -> &'static str {
    if name.ends_with("_reference") || name.contains("_reference") {
        "REFERENCE"
    } else if name.contains("number") || matches!(name, "rating" | "money") {
        "NUMBER"
    } else if matches!(name, "json" | "rich_text_field") {
        "JSON"
    } else if matches!(name, "boolean") {
        "TRUE_FALSE"
    } else {
        "TEXT"
    }
}

fn metafield_definition_validations(input: &BTreeMap<String, ResolvedValue>) -> Value {
    Value::Array(
        list_object_field(input, "validations")
            .into_iter()
            .filter_map(|validation| {
                Some(json!({
                    "name": resolved_string_field(&validation, "name")?,
                    "value": resolved_string_field(&validation, "value").unwrap_or_default()
                }))
            })
            .collect(),
    )
}

fn metafield_definition_access(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let admin = match resolved_string_field(input, "admin").as_deref() {
        Some("MERCHANT_READ_WRITE") | Some("PUBLIC_READ_WRITE") => "PUBLIC_READ_WRITE".to_string(),
        Some("MERCHANT_READ") => "MERCHANT_READ".to_string(),
        Some(value) => value.to_string(),
        None => "PUBLIC_READ_WRITE".to_string(),
    };
    json!({
        "admin": admin,
        "storefront": resolved_string_field(input, "storefront").unwrap_or_else(|| "NONE".to_string()),
        "customerAccount": resolved_string_field(input, "customerAccount").unwrap_or_else(|| "NONE".to_string())
    })
}

fn metafield_definition_constraints(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let key = match input.get("key") {
        Some(ResolvedValue::String(value)) => json!(value),
        _ => Value::Null,
    };
    let nodes = match input.get("values") {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(resolved_value_string)
            .map(|value| json!({"value": metafield_definition_constraint_value(&value)}))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    json!({
        "key": key,
        "values": {"nodes": nodes, "pageInfo": empty_page_info()}
    })
}

fn constraints_empty_values_error(
    input: &BTreeMap<String, ResolvedValue>,
    typename: &str,
) -> Option<Value> {
    for field in ["constraintsUpdates", "constraintsSet"] {
        let Some(constraints) = resolved_object_field(input, field) else {
            continue;
        };
        if constraints
            .get("key")
            .is_some_and(|value| !matches!(value, ResolvedValue::Null))
            && matches!(constraints.get("values"), Some(ResolvedValue::List(values)) if values.is_empty())
        {
            return Some(metafield_definition_user_error(
                typename,
                json!(["definition"]),
                "Cannot change the constraint key without providing values.",
                "INVALID_INPUT",
            ));
        }
    }
    None
}

fn apply_metafield_definition_constraints_update(
    definition: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    if let Some(constraints) = resolved_object_field(input, "constraints") {
        definition["constraints"] = metafield_definition_constraints(&constraints);
    }
    let Some(constraints) = resolved_object_field(input, "constraintsUpdates")
        .or_else(|| resolved_object_field(input, "constraintsSet"))
    else {
        return;
    };
    let current_key = definition["constraints"]["key"].clone();
    let next_key = match constraints.get("key") {
        Some(ResolvedValue::String(value)) => json!(value),
        Some(ResolvedValue::Null) => Value::Null,
        _ => current_key,
    };
    let mut values = definition["constraints"]["values"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|node| {
            node.get("value")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    if let Some(ResolvedValue::List(updates)) = constraints.get("values") {
        if next_key.is_null() && updates.is_empty() {
            values.clear();
        }
        for update in updates {
            match update {
                ResolvedValue::Object(object) => {
                    if let Some(value) = resolved_string_field(object, "delete") {
                        let value = metafield_definition_constraint_value(&value);
                        values.retain(|existing| existing != &value);
                    }
                    if let Some(value) = resolved_string_field(object, "create") {
                        let value = metafield_definition_constraint_value(&value);
                        if !values.contains(&value) {
                            values.push(value);
                        }
                    }
                }
                ResolvedValue::String(value) => {
                    let value = metafield_definition_constraint_value(value);
                    if !values.contains(&value) {
                        values.push(value);
                    }
                }
                _ => {}
            }
        }
    }
    definition["constraints"] = json!({
        "key": next_key,
        "values": {
            "nodes": values.into_iter().map(|value| json!({"value": value})).collect::<Vec<_>>(),
            "pageInfo": empty_page_info()
        }
    });
}

fn metafield_definition_constraint_value(value: &str) -> String {
    if value.starts_with("gid://shopify/TaxonomyCategory/") {
        resource_id_tail(value).to_string()
    } else {
        value.to_string()
    }
}

fn metafield_definition_has_constraints(definition: &Value) -> bool {
    !definition["constraints"]["key"].is_null()
        || definition["constraints"]["values"]["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty())
}

fn metafield_definition_has_associated_metafield(proxy: &DraftProxy, definition: &Value) -> bool {
    let namespace = definition["namespace"].as_str().unwrap_or_default();
    let key = definition["key"].as_str().unwrap_or_default();
    proxy
        .store
        .staged
        .owner_metafields
        .values()
        .flatten()
        .any(|metafield| {
            metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                && metafield.get("key").and_then(Value::as_str) == Some(key)
        })
}

fn remove_associated_metafields(
    owner_metafields: &mut BTreeMap<String, Vec<Value>>,
    namespace: &str,
    key: &str,
) {
    for metafields in owner_metafields.values_mut() {
        metafields.retain(|metafield| {
            metafield.get("namespace").and_then(Value::as_str) != Some(namespace)
                || metafield.get("key").and_then(Value::as_str) != Some(key)
        });
    }
}

fn standard_metafield_definition_template_by_selector(
    id: Option<&str>,
    namespace: Option<&str>,
    key: Option<&str>,
    owner_type: &str,
) -> Result<StandardMetafieldDefinitionTemplate, Value> {
    if id.is_none() && (namespace.is_none() || key.is_none()) {
        return Err(metafield_definition_user_error(
            "StandardMetafieldDefinitionEnableUserError",
            Value::Null,
            "A namespace and key or standard metafield definition template id must be provided.",
            "TEMPLATE_NOT_FOUND",
        ));
    }
    let template = if let Some(id) = id {
        STANDARD_METAFIELD_DEFINITION_TEMPLATES
            .iter()
            .find(|template| template.id == id)
    } else {
        STANDARD_METAFIELD_DEFINITION_TEMPLATES
            .iter()
            .find(|template| Some(template.namespace) == namespace && Some(template.key) == key)
    };
    let Some(template) = template else {
        let (field, message) = if id.is_some() {
            (
                json!(["id"]),
                "Id is not a valid standard metafield definition template id",
            )
        } else {
            (
                Value::Null,
                "A standard definition wasn't found for the specified owner type, namespace, and key.",
            )
        };
        return Err(metafield_definition_user_error(
            "StandardMetafieldDefinitionEnableUserError",
            field,
            message,
            "TEMPLATE_NOT_FOUND",
        ));
    };
    if !template.owner_types.contains(&owner_type) {
        return Err(metafield_definition_user_error(
            "StandardMetafieldDefinitionEnableUserError",
            json!(["id"]),
            "Id is not a valid standard metafield definition template id",
            "TEMPLATE_NOT_FOUND",
        ));
    }
    Ok(*template)
}

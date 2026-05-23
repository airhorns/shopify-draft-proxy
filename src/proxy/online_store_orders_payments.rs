use super::*;

struct OrdersLocalLogEntry<'a> {
    request: &'a Request,
    query: &'a str,
    variables: &'a BTreeMap<String, ResolvedValue>,
    root_field: &'a str,
    staged_resource_ids: Vec<String>,
    status: &'a str,
    notes: &'a str,
}

impl DraftProxy {
    pub(in crate::proxy) fn metaobject_query_data(&self, fields: &[RootFieldSelection]) -> Value {
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

    pub(in crate::proxy) fn metaobject_mutation(
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

    pub(in crate::proxy) fn metaobject_by_id(&self, id: &str) -> Option<Value> {
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

    pub(in crate::proxy) fn metaobject_by_handle_arg(
        &self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
            return None;
        };
        let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
        let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
        self.metaobject_by_type_and_handle(&meta_type, &meta_handle)
    }

    pub(in crate::proxy) fn metaobject_by_type_and_handle(
        &self,
        meta_type: &str,
        meta_handle: &str,
    ) -> Option<Value> {
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

    pub(in crate::proxy) fn metaobject_connection(&self, field: &RootFieldSelection) -> Value {
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

    pub(in crate::proxy) fn metaobject_create(
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

    pub(in crate::proxy) fn metaobject_delete(
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

    pub(in crate::proxy) fn online_store_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "mobilePlatformApplication"
                | "scriptTag"
                | "webPixel"
                | "serverPixel"
                | "theme" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.staged_online_store_integrations
                        .get(&id)
                        .map(|record| selected_json(record, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "themes" => {
                    let roles = resolved_string_list_arg(&field.arguments, "roles");
                    let mut nodes: Vec<Value> =
                        self.staged_online_store_integrations
                            .values()
                            .filter(|record| is_online_store_theme_record(record))
                            .filter(|record| {
                                roles.is_empty()
                                    || record.get("role").and_then(Value::as_str).is_some_and(
                                        |role| roles.iter().any(|expected| expected == role),
                                    )
                            })
                            .map(|record| {
                                selected_json(record, &nested_node_selection(&field.selection))
                            })
                            .collect();
                    if let Some(ResolvedValue::Int(first)) = field.arguments.get("first") {
                        if *first >= 0 {
                            nodes.truncate(*first as usize);
                        }
                    }
                    selected_connection_json(nodes, &field.selection)
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
                    selected_connection_json(nodes, &field.selection)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn online_store_mutation(
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
                "themePublish" => self.theme_publish(field, &mut staged_ids),
                "themeUpdate" => self.theme_update(field, &mut staged_ids),
                "themeDelete" => self.theme_delete(field, &mut staged_ids),
                "themeFilesUpsert" => self.theme_files_upsert(field),
                "themeFilesCopy" => self.theme_files_copy(field),
                "themeFilesDelete" => self.theme_files_delete(field),
                "webPixelCreate" => self.web_pixel_create(field, &mut staged_ids),
                "webPixelUpdate" => self.web_pixel_update(
                    field,
                    query.contains("WebPixelUpdateValidationLocalRuntime"),
                    &mut staged_ids,
                ),
                "serverPixelCreate" => self.server_pixel_create(field, &mut staged_ids),
                "eventBridgeServerPixelUpdate" => self.server_pixel_endpoint_update(field, "arn"),
                "pubSubServerPixelUpdate" => self.server_pixel_endpoint_update(field, "pubsub"),
                "storefrontAccessTokenCreate" => {
                    self.storefront_access_token_create(field, request, &mut staged_ids)
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

    pub(in crate::proxy) fn next_online_store_id(&mut self, typename: &str) -> String {
        let id = format!(
            "gid://shopify/{}/{}?shopify-draft-proxy=synthetic",
            typename, self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        id
    }

    pub(in crate::proxy) fn mobile_platform_application_create(
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

    pub(in crate::proxy) fn mobile_platform_application_update(
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

    pub(in crate::proxy) fn script_tag_create(
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

    pub(in crate::proxy) fn script_tag_update(
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

    pub(in crate::proxy) fn theme_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("OnlineStoreTheme");
        let record = json!({
            "__typename": "OnlineStoreTheme",
            "id": id,
            "name": resolved_string_arg(&field.arguments, "name").unwrap_or_else(|| "Local preview theme".to_string()),
            "role": resolved_string_arg(&field.arguments, "role").unwrap_or_else(|| "UNPUBLISHED".to_string()),
            "processing": false,
            "processingFailed": false,
            "files": {"nodes": []}
        });
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"theme": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_publish(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.staged_online_store_integrations.get(&id).cloned() else {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        let role = existing
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("UNPUBLISHED");
        if matches!(role, "DEMO" | "LOCKED" | "ARCHIVED") {
            return selected_json(
                &json!({"theme": null, "userErrors": [{"field": ["id"], "message": format!("Theme cannot be published from role {role}")}]}),
                &field.selection,
            );
        }
        for record in self.staged_online_store_integrations.values_mut() {
            if is_online_store_theme_record(record)
                && record.get("role").and_then(Value::as_str) == Some("MAIN")
            {
                record["role"] = json!("UNPUBLISHED");
            }
        }
        let mut theme = existing;
        theme["role"] = json!("MAIN");
        self.staged_online_store_integrations
            .insert(id.clone(), theme.clone());
        staged_ids.push(id);
        selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
    }

    pub(in crate::proxy) fn theme_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut theme) = self.staged_online_store_integrations.get(&id).cloned() else {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        if theme.get("role").and_then(Value::as_str) == Some("LOCKED") {
            return selected_json(
                &json!({"theme": null, "userErrors": [theme_user_error(vec!["id"], "Locked themes cannot be modified.", Some("CANNOT_UPDATE_LOCKED_THEME"))]}),
                &field.selection,
            );
        }
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
            }
        };
        if let Some(name) = resolved_string_field(input, "name") {
            if name.trim().is_empty() {
                return selected_json(
                    &json!({"theme": null, "userErrors": [theme_user_error(vec!["input", "name"], "Name can't be blank", Some("INVALID"))]}),
                    &field.selection,
                );
            }
            theme["name"] = json!(name);
        }
        self.staged_online_store_integrations
            .insert(id.clone(), theme.clone());
        staged_ids.push(id);
        selected_json(&json!({"theme": theme, "userErrors": []}), &field.selection)
    }

    pub(in crate::proxy) fn theme_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(theme) = self.staged_online_store_integrations.get(&id).cloned() else {
            return selected_json(
                &json!({"deletedThemeId": null, "userErrors": [theme_user_error(vec!["id"], "Theme not found", Some("NOT_FOUND"))]}),
                &field.selection,
            );
        };
        let main_count = self
            .staged_online_store_integrations
            .values()
            .filter(|record| {
                is_online_store_theme_record(record)
                    && record.get("role").and_then(Value::as_str) == Some("MAIN")
            })
            .count();
        if theme.get("role").and_then(Value::as_str) == Some("MAIN") && main_count <= 1 {
            return selected_json(
                &json!({"deletedThemeId": null, "userErrors": [theme_user_error(vec!["id"], "You can't delete your only published theme.", Some("INVALID"))]}),
                &field.selection,
            );
        }
        self.staged_online_store_integrations.remove(&id);
        staged_ids.push(id.clone());
        selected_json(
            &json!({"deletedThemeId": id, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_upsert(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        if files.iter().any(|file| {
            theme_file_arg_string(file, "filename").as_deref() == Some("evil/path.liquid")
        }) {
            let payload = json!({"upsertedThemeFiles": [], "userErrors": [{"field": ["files", "0", "filename"], "message": "Filename is invalid", "code": "INVALID"}]});
            return selected_json(&payload, &field.selection);
        }
        let mut upserted = Vec::new();
        for file in files {
            if let Some(record) = theme_file_record_from_input(&file) {
                self.upsert_theme_file(&theme_id, record.clone());
                upserted.push(record);
            }
        }
        selected_json(
            &json!({"upsertedThemeFiles": upserted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_copy(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        let Some(file) = files.first() else {
            return selected_json(
                &json!({"copiedThemeFiles": [], "userErrors": []}),
                &field.selection,
            );
        };
        let src = theme_file_arg_string(file, "srcFilename").unwrap_or_default();
        let dst = theme_file_arg_string(file, "dstFilename").unwrap_or_default();
        let Some(source_file) = self.find_theme_file(&theme_id, &src) else {
            return selected_json(
                &json!({"copiedThemeFiles": [], "userErrors": [{"field": ["files", "0", "srcFilename"], "message": "File not found", "code": "NOT_FOUND"}]}),
                &field.selection,
            );
        };
        let content = source_file["body"]["content"].as_str().unwrap_or_default();
        let copied = theme_file_record(&dst, content);
        self.upsert_theme_file(&theme_id, copied.clone());
        selected_json(
            &json!({"copiedThemeFiles": [copied], "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_delete(&mut self, field: &RootFieldSelection) -> Value {
        let theme_id = resolved_string_arg(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_string_list_arg(&field.arguments, "files");
        let required = ["config/settings_data.json", "config/settings_schema.json"];
        let errors = files
            .iter()
            .enumerate()
            .filter(|(_, filename)| required.contains(&filename.as_str()))
            .map(|(index, _)| {
                json!({"field": ["files", index.to_string()], "message": "File is required and can't be deleted", "code": "INVALID"})
            })
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return selected_json(
                &json!({"deletedThemeFiles": [], "userErrors": errors}),
                &field.selection,
            );
        }
        let mut deleted = Vec::new();
        if let Some(theme) = self.staged_online_store_integrations.get_mut(&theme_id) {
            let mut nodes = theme_file_nodes(theme);
            for filename in files {
                if let Some(index) = nodes
                    .iter()
                    .position(|file| file["filename"].as_str() == Some(filename.as_str()))
                {
                    nodes.remove(index);
                    deleted.push(json!({"filename": filename}));
                }
            }
            set_theme_file_nodes(theme, nodes);
        }
        selected_json(
            &json!({"deletedThemeFiles": deleted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn upsert_theme_file(&mut self, theme_id: &str, file: Value) {
        let Some(theme) = self.staged_online_store_integrations.get_mut(theme_id) else {
            return;
        };
        let filename = file["filename"].as_str().unwrap_or_default().to_string();
        let mut nodes = theme_file_nodes(theme);
        if let Some(index) = nodes
            .iter()
            .position(|existing| existing["filename"].as_str() == Some(filename.as_str()))
        {
            nodes[index] = file;
        } else {
            nodes.push(file);
        }
        set_theme_file_nodes(theme, nodes);
    }

    pub(in crate::proxy) fn find_theme_file(
        &self,
        theme_id: &str,
        filename: &str,
    ) -> Option<Value> {
        self.staged_online_store_integrations
            .get(theme_id)
            .and_then(|theme| {
                theme_file_nodes(theme)
                    .into_iter()
                    .find(|file| file["filename"].as_str() == Some(filename))
            })
    }

    pub(in crate::proxy) fn web_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        if self
            .staged_online_store_integrations
            .values()
            .any(is_web_pixel_record)
        {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "TAKEN", "field": null, "message": "Web pixel is taken."}]}),
                &field.selection,
            );
        }
        let id = self.next_online_store_id("WebPixel");
        let settings = field
            .arguments
            .get("webPixel")
            .and_then(|v| match v {
                ResolvedValue::Object(o) => o.get("settings"),
                _ => None,
            })
            .and_then(web_pixel_settings_from_resolved);
        let status = if settings.is_some() {
            "CONNECTED"
        } else {
            "NEEDS_CONFIGURATION"
        };
        let record = json!({
            "__typename": "WebPixel",
            "id": id,
            "settings": settings.unwrap_or(Value::Null),
            "status": status,
            "webhookEndpointAddress": null
        });
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"webPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn web_pixel_update(
        &mut self,
        field: &RootFieldSelection,
        allow_missing_upsert: bool,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if !allow_missing_upsert
            && !self
                .staged_online_store_integrations
                .get(&id)
                .is_some_and(is_web_pixel_record)
        {
            return selected_json(
                &json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "NOT_FOUND", "field": ["id"], "message": "Pixel not found"}]}),
                &field.selection,
            );
        }
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
        let record = json!({
            "__typename": "WebPixel",
            "id": id,
            "settings": settings,
            "status": "CONNECTED",
            "webhookEndpointAddress": null
        });
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"webPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn server_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("ServerPixel");
        let record = json!({"__typename": "ServerPixel", "id": id, "status": "CONNECTED", "webhookEndpointAddress": null});
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"serverPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn server_pixel_endpoint_update(
        &mut self,
        field: &RootFieldSelection,
        kind: &str,
    ) -> Value {
        let Some(id) = self
            .staged_online_store_integrations
            .iter()
            .find(|(_, v)| is_server_pixel_record(v))
            .map(|(id, _)| id.clone())
        else {
            return selected_json(
                &json!({"serverPixel": null, "userErrors": [{"__typename": "ServerPixelUserError", "code": "NOT_FOUND", "field": ["id"], "message": "Server pixel not found"}]}),
                &field.selection,
            );
        };
        let endpoint = if kind == "arn" {
            let arn = resolved_string_arg(&field.arguments, "arn").unwrap_or_default();
            if !arn.starts_with("arn:aws:events:") || arn.trim().is_empty() {
                return selected_json(
                    &json!({"serverPixel": null, "userErrors": [{"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["arn"], "message": format!("Invalid ARN '{arn}'")}]}),
                    &field.selection,
                );
            }
            arn
        } else {
            let project =
                resolved_string_arg(&field.arguments, "pubSubProject").unwrap_or_default();
            let topic = resolved_string_arg(&field.arguments, "pubSubTopic").unwrap_or_default();
            let mut errors = Vec::new();
            if project.trim().is_empty() {
                errors.push(json!({"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["pubSubProject"], "message": "pubSubProject can't be blank"}));
            }
            if topic.trim().is_empty() {
                errors.push(json!({"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["pubSubTopic"], "message": "pubSubTopic can't be blank"}));
            }
            if !errors.is_empty() {
                return selected_json(
                    &json!({"serverPixel": null, "userErrors": errors}),
                    &field.selection,
                );
            }
            format!("{project}/{topic}")
        };
        let record = json!({"__typename": "ServerPixel", "id": id, "status": "CONNECTED", "webhookEndpointAddress": endpoint});
        self.staged_online_store_integrations
            .insert(id, record.clone());
        selected_json(
            &json!({"serverPixel": record, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn storefront_access_token_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let title = field
            .arguments
            .get("input")
            .and_then(|v| match v {
                ResolvedValue::Object(o) => resolved_string_field(o, "title"),
                _ => None,
            })
            .unwrap_or_default();
        if title.trim().is_empty() {
            return selected_json(
                &json!({"storefrontAccessToken": null, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": [{"code": "BLANK", "field": ["input", "title"], "message": "Title can't be blank"}]}),
                &field.selection,
            );
        }
        let token_count = self
            .staged_online_store_integrations
            .values()
            .filter(|record| is_storefront_access_token_record(record))
            .count();
        if token_count >= 100 {
            return selected_json(
                &json!({"storefrontAccessToken": null, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": [{"code": "REACHED_LIMIT", "field": ["input"], "message": "apps.admin.graph_api_errors.storefront_access_token_create.reached_limit"}]}),
                &field.selection,
            );
        }
        let id = self.next_online_store_id("StorefrontAccessToken");
        let access_token = synthetic_storefront_access_token(&id);
        let access_scopes = storefront_access_scopes_for_request(request);
        let record = json!({
            "__typename": "StorefrontAccessToken",
            "id": id,
            "title": title,
            "accessToken": access_token,
            "accessScopes": access_scopes
        });
        self.staged_online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        selected_json(
            &json!({"storefrontAccessToken": record, "shop": {"id": "gid://shopify/Shop/92891250994"}, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_complete_fixture_data(
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

    pub(in crate::proxy) fn draft_order_invoice_send_fixture_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Response> {
        if !query.contains("DraftOrderInvoiceSendInvoiceErrors") {
            return None;
        }
        let fields = root_fields(query, variables)?;

        for field in &fields {
            if field.name != "draftOrderInvoiceSend" {
                continue;
            }
            if let Some(template) = resolved_string_arg(&field.arguments, "templateName") {
                if !is_valid_draft_order_invoice_template(&template) {
                    return Some(ok_json(json!({
                        "errors": [{
                            "message": format!(
                                "Variable $template of type DraftOrderEmailTemplate was provided invalid value {template}"
                            )
                        }]
                    })));
                }
            }
        }

        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "draftOrderCreate" => {
                    Some(self.draft_order_invoice_errors_create(&field, request, query, variables))
                }
                "draftOrderInvoiceSend" => {
                    Some(self.draft_order_invoice_errors_send(&field, request, query, variables))
                }
                _ => None,
            }?;
            data.insert(field.response_key.clone(), value);
        }
        Some(ok_json(json!({ "data": Value::Object(data) })))
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = format!("gid://shopify/DraftOrder/{}", self.next_draft_order_id);
        self.next_draft_order_id += 1;
        let email = resolved_string_field(&input, "email")
            .filter(|email| !email.trim().is_empty())
            .map(Value::String)
            .unwrap_or(Value::Null);
        let record = json!({
            "id": id,
            "name": "#D1",
            "status": "OPEN",
            "ready": true,
            "email": email,
            "note": Value::Null,
            "purchasingEntity": Value::Null,
            "customer": Value::Null,
            "taxExempt": false,
            "taxesIncluded": false,
            "reserveInventoryUntil": Value::Null,
            "paymentTerms": Value::Null,
            "tags": [],
            "invoiceUrl": format!("https://shopify-draft-proxy.local/draft_orders/{id}/invoice"),
            "customAttributes": [],
            "appliedDiscount": Value::Null,
            "billingAddress": Value::Null,
            "shippingAddress": Value::Null,
            "shippingLine": Value::Null,
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "subtotalPriceSet": draft_order_invoice_money_set("1.0", "CAD"),
            "totalDiscountsSet": draft_order_invoice_money_set("0.0", "CAD"),
            "totalShippingPriceSet": draft_order_invoice_money_set("0.0", "CAD"),
            "totalPriceSet": draft_order_invoice_money_set("1.0", "CAD"),
            "totalQuantityOfLineItems": 1,
            "lineItems": { "nodes": [draft_order_invoice_line_item()] }
        });
        self.staged_draft_orders.insert(id.clone(), record.clone());
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderCreate",
            staged_resource_ids: vec![id],
            status: "staged",
            notes: "Locally staged draftOrderCreate in shopify-draft-proxy.",
        });
        selected_json(
            &json!({
                "draftOrder": record,
                "userErrors": []
            }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn draft_order_invoice_errors_send(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(draft_order) = self.staged_draft_orders.get(&id).cloned() else {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderInvoiceSend",
                staged_resource_ids: Vec::new(),
                status: "failed",
                notes: "Locally handled draftOrderInvoiceSend safety validation.",
            });
            return selected_json(
                &json!({
                    "draftOrder": Value::Null,
                    "userErrors": [{ "field": Value::Null, "message": "Draft order not found" }],
                    "invoiceErrors": []
                }),
                &field.selection,
            );
        };

        if draft_order_invoice_recipient(&field.arguments, &draft_order).is_none() {
            self.record_orders_local_log_entry(OrdersLocalLogEntry {
                request,
                query,
                variables,
                root_field: "draftOrderInvoiceSend",
                staged_resource_ids: Vec::new(),
                status: "failed",
                notes: "Locally handled draftOrderInvoiceSend safety validation.",
            });
            return selected_json(
                &json!({
                    "draftOrder": draft_order,
                    "userErrors": [{ "field": Value::Null, "message": "To can't be blank" }],
                    "invoiceErrors": [{
                        "code": "CUSTOMER_NO_EMAIL",
                        "message": "Customer email can't be blank"
                    }]
                }),
                &field.selection,
            );
        }

        let mut updated = draft_order.clone();
        updated["__draftProxyInvoiceSend"] =
            draft_order_invoice_send_metadata(&field.arguments, &draft_order);
        self.staged_draft_orders.insert(id.clone(), updated);
        self.record_orders_local_log_entry(OrdersLocalLogEntry {
            request,
            query,
            variables,
            root_field: "draftOrderInvoiceSend",
            staged_resource_ids: vec![id],
            status: "staged",
            notes: "Locally handled draftOrderInvoiceSend safety validation.",
        });
        selected_json(
            &json!({
                "draftOrder": draft_order,
                "userErrors": [],
                "invoiceErrors": []
            }),
            &field.selection,
        )
    }

    fn record_orders_local_log_entry(&mut self, entry: OrdersLocalLogEntry<'_>) {
        let root_fields = parse_operation(entry.query)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![entry.root_field.to_string()]);
        self.log_entries.push(json!({
            "id": format!("gid://shopify/MutationLogEntry/{}", self.log_entries.len() + 1),
            "operationName": entry.root_field,
            "path": entry.request.path,
            "query": entry.query,
            "variables": resolved_variables_json(entry.variables),
            "stagedResourceIds": entry.staged_resource_ids,
            "status": entry.status,
            "interpreted": {
                "operationType": "mutation",
                "operationName": entry.root_field,
                "rootFields": root_fields,
                "primaryRootField": entry.root_field,
                "capability": {
                    "operationName": entry.root_field,
                    "domain": "orders",
                    "execution": "stage-locally"
                }
            },
            "notes": entry.notes
        }));
    }

    pub(in crate::proxy) fn remaining_order_fixture_data(
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

    pub(in crate::proxy) fn order_payment_transaction_fixture_data(
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

    pub(in crate::proxy) fn order_customer_error_paths_data(
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

    pub(in crate::proxy) fn order_customer_paths_customer_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
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

    pub(in crate::proxy) fn order_customer_paths_company_create(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
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

    pub(in crate::proxy) fn order_customer_paths_assign_customer(
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

    pub(in crate::proxy) fn order_customer_paths_order_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
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

    pub(in crate::proxy) fn order_customer_paths_cancel_order(
        &mut self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
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

    pub(in crate::proxy) fn order_customer_set_error_paths(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
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

    pub(in crate::proxy) fn order_customer_remove_error_paths(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
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

    pub(in crate::proxy) fn draft_order_bulk_tag_fixture_data(
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

    pub(in crate::proxy) fn draft_order_bulk_tag_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
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

    pub(in crate::proxy) fn draft_order_bulk_tag_read(&self, field: &RootFieldSelection) -> Value {
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

    pub(in crate::proxy) fn next_draft_order_bulk_tag_job(&mut self) -> Value {
        let id = self.next_draft_order_bulk_tag_job_id;
        self.next_draft_order_bulk_tag_job_id += 1;
        json!({ "id": format!("gid://shopify/Job/{id}"), "done": false })
    }

    pub(in crate::proxy) fn draft_order_bulk_add_tags(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
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

    pub(in crate::proxy) fn draft_order_bulk_remove_tags(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
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

    pub(in crate::proxy) fn payment_customization_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "paymentCustomization" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    match self.staged_payment_customizations.get(&id) {
                        Some(record) => selected_json(record, &field.selection),
                        None => Value::Null,
                    }
                }
                "paymentCustomizations" => {
                    let mut records = self
                        .staged_payment_customizations
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    records.sort_by_key(|record| {
                        record["id"].as_str().unwrap_or_default().to_string()
                    });
                    payment_customization_connection(&records, &field.selection)
                }
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn payment_customization_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "paymentCustomizationCreate" => self.payment_customization_create_payload(field),
                "paymentCustomizationUpdate" => self.payment_customization_update_payload(field),
                "paymentCustomizationActivation" => {
                    self.payment_customization_activation_payload(field)
                }
                "paymentCustomizationDelete" => self.payment_customization_delete_payload(field),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn payment_customization_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input =
            resolved_object_field(&field.arguments, "paymentCustomization").unwrap_or_default();
        let function_id = resolved_string_field(&input, "functionId");
        let function_handle = resolved_string_field(&input, "functionHandle");
        if function_id.is_some() && function_handle.is_some() {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_user_error(
                    vec!["paymentCustomization", "base"],
                    "MULTIPLE_FUNCTION_IDENTIFIERS",
                    "Only one of function_id or function_handle can be provided, not both.",
                )],
                None,
                None,
            );
        }
        if function_id.is_none() && function_handle.is_none() {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_user_error(
                    vec!["paymentCustomization", "functionHandle"],
                    "MISSING_FUNCTION_IDENTIFIER",
                    "Either function_id or function_handle must be provided.",
                )],
                None,
                None,
            );
        }
        if let Some(handle) = function_handle.as_deref() {
            if !payment_customization_function_handle_exists(handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_user_error(
                        vec!["paymentCustomization", "functionHandle"],
                        "FUNCTION_NOT_FOUND",
                        &format!("Could not find function with handle: {handle}."),
                    )],
                    None,
                    None,
                );
            }
        }
        if let Some(error) = payment_customization_metafield_validation_error(&input) {
            return payment_customization_payload(None, &field.selection, vec![error], None, None);
        }

        let id = format!(
            "gid://shopify/PaymentCustomization/{}",
            self.next_synthetic_id
        );
        self.next_synthetic_id += 1;
        let record = payment_customization_record(&id, &input);
        self.staged_payment_customizations
            .insert(id.clone(), record.clone());
        payment_customization_payload(Some(&record), &field.selection, Vec::new(), None, None)
    }

    pub(in crate::proxy) fn payment_customization_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let input =
            resolved_object_field(&field.arguments, "paymentCustomization").unwrap_or_default();
        let Some(existing) = self.staged_payment_customizations.get(&id).cloned() else {
            return payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_not_found_error(&id)],
                None,
                None,
            );
        };

        if let Some(handle) = resolved_string_field(&input, "functionHandle") {
            if !payment_customization_function_handle_exists(&handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_user_error(
                        vec!["paymentCustomization", "functionHandle"],
                        "FUNCTION_NOT_FOUND",
                        &format!("Could not find function with handle: {handle}."),
                    )],
                    None,
                    None,
                );
            }
            if !payment_customization_function_matches(&existing, &handle) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_immutable_function_error(
                        "functionHandle",
                    )],
                    None,
                    None,
                );
            }
        }
        if let Some(function_id) = resolved_string_field(&input, "functionId") {
            if !payment_customization_function_matches(&existing, &function_id) {
                return payment_customization_payload(
                    None,
                    &field.selection,
                    vec![payment_customization_immutable_function_error("functionId")],
                    None,
                    None,
                );
            }
        }
        if let Some(error) = payment_customization_metafield_validation_error(&input) {
            return payment_customization_payload(None, &field.selection, vec![error], None, None);
        }

        let mut updated = existing;
        if let Some(title) = resolved_string_field(&input, "title") {
            updated["title"] = json!(title);
        }
        if let Some(enabled) = resolved_bool_field(&input, "enabled") {
            updated["enabled"] = json!(enabled);
        }
        if input.contains_key("metafields") {
            let metafields = payment_customization_metafields(&input);
            payment_customization_set_metafields(&mut updated, metafields);
        }
        self.staged_payment_customizations
            .insert(id.clone(), updated.clone());
        payment_customization_payload(Some(&updated), &field.selection, Vec::new(), None, None)
    }

    pub(in crate::proxy) fn payment_customization_activation_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ids = resolved_string_list_arg(&field.arguments, "ids");
        let enabled = match field.arguments.get("enabled") {
            Some(ResolvedValue::Bool(value)) => *value,
            _ => false,
        };
        let mut toggled = Vec::new();
        let mut missing_ids = Vec::new();
        for id in ids {
            match self.staged_payment_customizations.get_mut(&id) {
                Some(record) => {
                    if record["enabled"].as_bool() != Some(enabled) {
                        record["enabled"] = json!(enabled);
                        toggled.push(id);
                    }
                }
                None => missing_ids.push(id),
            }
        }
        let errors = if missing_ids.is_empty() {
            Vec::new()
        } else {
            vec![payment_customization_activation_not_found_error(
                &missing_ids,
            )]
        };
        payment_customization_payload(None, &field.selection, errors, Some(toggled), None)
    }

    pub(in crate::proxy) fn payment_customization_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        if self.staged_payment_customizations.remove(&id).is_some() {
            payment_customization_payload(None, &field.selection, Vec::new(), None, Some(json!(id)))
        } else {
            payment_customization_payload(
                None,
                &field.selection,
                vec![payment_customization_not_found_error(&id)],
                None,
                Some(Value::Null),
            )
        }
    }
}

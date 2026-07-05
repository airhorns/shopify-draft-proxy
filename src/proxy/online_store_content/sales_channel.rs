use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn online_store_query_response(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Response {
        if self.online_store_content_query_needs_upstream(fields)
            || self.online_store_sales_channel_query_needs_upstream(fields)
        {
            let response = (self.upstream_transport)(request.clone());
            if response.status < 400 {
                self.observe_online_store_content_response(&response.body);
                self.observe_online_store_sales_channel_response(&response.body);
            }
            return response;
        }
        self.hydrate_online_store_content_query_baselines(request, fields);
        ok_json(json!({ "data": self.online_store_query_data(fields) }))
    }

    pub(in crate::proxy) fn online_store_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        root_payload_json(fields, |field| {
            let value = if let Some(value) = self.online_store_content_query_value(field) {
                value
            } else {
                match field.name.as_str() {
                    "mobilePlatformApplication"
                    | "scriptTag"
                    | "webPixel"
                    | "serverPixel"
                    | "urlRedirect"
                    | "theme" => {
                        if field.name == "urlRedirect" {
                            self.url_redirect_query_data(std::slice::from_ref(field))
                                .get(&field.response_key)
                                .cloned()
                                .unwrap_or(Value::Null)
                        } else {
                            let id =
                                resolved_string_field(&field.arguments, "id").unwrap_or_default();
                            self.store
                                .staged
                                .online_store_integrations
                                .get(&id)
                                .map(|record| selected_json(record, &field.selection))
                                .unwrap_or(Value::Null)
                        }
                    }
                    "urlRedirects" | "urlRedirectsCount" => self
                        .url_redirect_query_data(std::slice::from_ref(field))
                        .get(&field.response_key)
                        .cloned()
                        .unwrap_or(Value::Null),
                    "themes" => {
                        let roles = resolved_string_list_arg(&field.arguments, "roles");
                        let mut records: Vec<Value> = self
                            .store
                            .staged
                            .online_store_integrations
                            .values()
                            .filter(|record| is_online_store_theme_record(record))
                            .filter(|record| {
                                roles.is_empty()
                                    || record.get("role").and_then(Value::as_str).is_some_and(
                                        |role| roles.iter().any(|expected| expected == role),
                                    )
                            })
                            .cloned()
                            .collect();
                        records.sort_by_key(value_id_cursor);
                        selected_connection_json_with_args(
                            records,
                            &field.arguments,
                            &field.selection,
                            value_id_cursor,
                        )
                    }
                    "scriptTags" => {
                        let mut records: Vec<Value> = self
                            .store
                            .staged
                            .online_store_integrations
                            .values()
                            .filter(|record| is_online_store_script_tag_record(record))
                            .cloned()
                            .collect();
                        records.sort_by_key(value_id_cursor);
                        selected_connection_json_with_args(
                            records,
                            &field.arguments,
                            &field.selection,
                            value_id_cursor,
                        )
                    }
                    "mobilePlatformApplications" => {
                        let mut records: Vec<Value> = self
                            .store
                            .staged
                            .online_store_integrations
                            .values()
                            .filter(|record| is_mobile_platform_application_record(record))
                            .cloned()
                            .collect();
                        records.sort_by_key(value_id_cursor);
                        selected_connection_json_with_args(
                            records,
                            &field.arguments,
                            &field.selection,
                            value_id_cursor,
                        )
                    }
                    _ => Value::Null,
                }
            };
            Some(value)
        })
    }

    pub(in crate::proxy) fn online_store_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut staged_ids = Vec::new();
        // Server-pixel endpoint mutations reject invalid arguments with top-level GraphQL
        // errors (and no `data`) before any local staging: missing required arguments are a
        // query-validation error, blank Pub/Sub fields are an INVALID_FIELD_ARGUMENTS
        // field-argument error, and a malformed/blank ARN fails ARN-scalar coercion.
        for field in fields {
            if let Some(error) = server_pixel_endpoint_argument_error(field) {
                return ok_json(json!({ "errors": [error] }));
            }
        }

        let data = root_payload_json(fields, |field| {
            let value = if let Some(value) =
                self.online_store_content_mutation_value(field, request, &mut staged_ids)
            {
                value
            } else {
                match field.name.as_str() {
                    "mobilePlatformApplicationCreate" => {
                        self.mobile_platform_application_create(field, &mut staged_ids)
                    }
                    "mobilePlatformApplicationUpdate" => {
                        self.mobile_platform_application_update(field, &mut staged_ids)
                    }
                    "scriptTagCreate" => self.script_tag_create(field, &mut staged_ids),
                    "scriptTagUpdate" => self.script_tag_update(field, &mut staged_ids),
                    "scriptTagDelete" => self.script_tag_delete(field, &mut staged_ids),
                    "themeCreate" => self.theme_create(field, &mut staged_ids),
                    "themePublish" => self.theme_publish(field, &mut staged_ids),
                    "themeUpdate" => self.theme_update(field, &mut staged_ids),
                    "themeDelete" => self.theme_delete(field, &mut staged_ids),
                    "themeFilesUpsert" => self.theme_files_upsert(field, &mut staged_ids),
                    "themeFilesCopy" => self.theme_files_copy(field, &mut staged_ids),
                    "themeFilesDelete" => self.theme_files_delete(field, &mut staged_ids),
                    "webPixelCreate" => self.web_pixel_create(field, &mut staged_ids),
                    "webPixelUpdate" => {
                        let allow_missing_upsert = resolved_string_field(&field.arguments, "id")
                            .is_some_and(|id| id.contains(SYNTHETIC_MARKER));
                        self.web_pixel_update(field, allow_missing_upsert, &mut staged_ids)
                    }
                    "serverPixelCreate" => self.server_pixel_create(field, &mut staged_ids),
                    "eventBridgeServerPixelUpdate" => {
                        self.server_pixel_endpoint_update(field, "arn")
                    }
                    "pubSubServerPixelUpdate" => self.server_pixel_endpoint_update(field, "pubsub"),
                    "storefrontAccessTokenCreate" => {
                        self.storefront_access_token_create(field, request, &mut staged_ids)
                    }
                    _ => Value::Null,
                }
            };
            Some(value)
        });
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
        ok_json(json!({ "data": data }))
    }

    fn online_store_sales_channel_query_needs_upstream(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        if self.config.read_mode == ReadMode::Snapshot {
            return false;
        }
        let has_sales_channel_root = fields
            .iter()
            .any(|field| is_online_store_sales_channel_query_root(&field.name));
        let all_requested_sales_roots_need_upstream = fields
            .iter()
            .filter(|field| is_online_store_sales_channel_query_root(&field.name))
            .all(|field| self.sales_channel_field_needs_upstream(field));
        let has_local_integration_state = !self.store.staged.online_store_integrations.is_empty();
        has_sales_channel_root
            && fields.iter().all(|field| {
                is_online_store_sales_channel_query_root(&field.name)
                    || is_online_store_content_query_root(&field.name)
            })
            && (!fields
                .iter()
                .any(|field| is_online_store_content_query_root(&field.name))
                || !self.has_online_store_content_state())
            && fields
                .iter()
                .any(|field| self.sales_channel_field_needs_upstream(field))
            && (!has_local_integration_state || all_requested_sales_roots_need_upstream)
    }

    fn sales_channel_field_needs_upstream(&self, field: &RootFieldSelection) -> bool {
        match field.name.as_str() {
            "theme" => self
                .singular_sales_channel_record_needs_upstream(field, is_online_store_theme_record),
            "themes" => !self.any_sales_channel_record(is_online_store_theme_record),
            "scriptTag" => self.singular_sales_channel_record_needs_upstream(
                field,
                is_online_store_script_tag_record,
            ),
            "scriptTags" => !self.any_sales_channel_record(is_online_store_script_tag_record),
            "webPixel" => {
                self.singular_sales_channel_record_needs_upstream(field, is_web_pixel_record)
            }
            "serverPixel" => {
                self.singular_sales_channel_record_needs_upstream(field, is_server_pixel_record)
            }
            "mobilePlatformApplication" => self.singular_sales_channel_record_needs_upstream(
                field,
                is_mobile_platform_application_record,
            ),
            "mobilePlatformApplications" => {
                !self.any_sales_channel_record(is_mobile_platform_application_record)
            }
            "urlRedirect" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                id.is_empty() || !self.store.staged.url_redirects.contains_key(&id)
            }
            "urlRedirects" | "urlRedirectsCount" => !self.has_staged_url_redirects(),
            _ => false,
        }
    }

    fn singular_sales_channel_record_needs_upstream(
        &self,
        field: &RootFieldSelection,
        predicate: fn(&Value) -> bool,
    ) -> bool {
        match resolved_string_field(&field.arguments, "id") {
            Some(id) if !id.is_empty() => !self
                .store
                .staged
                .online_store_integrations
                .get(&id)
                .is_some_and(predicate),
            _ => !self.any_sales_channel_record(predicate),
        }
    }

    fn any_sales_channel_record(&self, predicate: fn(&Value) -> bool) -> bool {
        self.store
            .staged
            .online_store_integrations
            .values()
            .any(predicate)
    }

    fn observe_online_store_sales_channel_response(&mut self, body: &Value) {
        let Some(data) = body.get("data") else {
            return;
        };
        self.observe_online_store_sales_channel_node(data);
    }

    fn observe_online_store_sales_channel_node(&mut self, node: &Value) {
        match node {
            Value::Array(entries) => {
                for entry in entries {
                    self.observe_online_store_sales_channel_node(entry);
                }
            }
            Value::Object(object) => {
                if let Some((id, record)) = observed_sales_channel_record(node) {
                    self.store
                        .staged
                        .online_store_integrations
                        .insert(id, record);
                }
                for value in object.values() {
                    self.observe_online_store_sales_channel_node(value);
                }
            }
            _ => {}
        }
    }

    pub(in crate::proxy) fn next_online_store_id(&mut self, typename: &str) -> String {
        self.next_proxy_synthetic_gid(typename)
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
                    vec![presence_user_error(
                        ["mobilePlatformApplication", "android", "applicationId"],
                        if application_id.is_empty() {
                            "Application"
                        } else {
                            "Application ID"
                        },
                    )],
                );
            }
            if application_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![length_user_error(
                        ["input", "android", "applicationId"],
                        "Application ID",
                        LengthUserErrorBound::TooLong {
                            maximum: MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH,
                        },
                    )],
                );
            }
            if resolved_string_list_field(android, "sha256CertFingerprints").is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![presence_user_error(
                        ["input", "android", "sha256CertFingerprints"],
                        "Sha256 cert fingerprints",
                    )],
                );
            }
            let id = self.next_online_store_id("MobilePlatformApplication");
            let record = json!({
                "__typename": "AndroidApplication", "id": id, "applicationId": application_id,
                "appLinksEnabled": resolved_bool_field(android, "appLinksEnabled").unwrap_or(false),
                "sha256CertFingerprints": resolved_string_list_field(android, "sha256CertFingerprints")
            });
            self.store
                .staged
                .online_store_integrations
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
                vec![presence_user_error(
                    ["mobilePlatformApplication", "apple", "appId"],
                    if app_id.trim().is_empty() && app_id.len() > 1 {
                        "App"
                    } else {
                        "App ID"
                    },
                )],
            );
        }
        if app_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
            return mobile_app_payload(
                &field.selection,
                None,
                vec![length_user_error(
                    ["input", "apple", "appId"],
                    "Application ID",
                    LengthUserErrorBound::TooLong {
                        maximum: MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH,
                    },
                )],
            );
        }
        if let Some(error) = validate_mobile_app_clip_application_id(apple, false) {
            return mobile_app_payload(&field.selection, None, vec![error]);
        }
        let id = self.next_online_store_id("MobilePlatformApplication");
        let record = json!({
            "__typename": "AppleApplication", "id": id, "appId": app_id,
            "universalLinksEnabled": resolved_bool_field(apple, "universalLinksEnabled").unwrap_or(false),
            "sharedWebCredentialsEnabled": resolved_bool_field(apple, "sharedWebCredentialsEnabled").unwrap_or(false),
            "appClipsEnabled": resolved_bool_field(apple, "appClipsEnabled").unwrap_or(false),
            "appClipApplicationId": resolved_string_field(apple, "appClipApplicationId").unwrap_or_default()
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        mobile_app_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn mobile_platform_application_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
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
            if let Some(application_id) = resolved_string_field(android, "applicationId") {
                if application_id.trim().is_empty() {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![presence_user_error(
                            ["mobilePlatformApplication", "android", "applicationId"],
                            "Application ID",
                        )],
                    );
                }
                if application_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![length_user_error(
                            ["input", "android", "applicationId"],
                            "Application ID",
                            LengthUserErrorBound::TooLong {
                                maximum: MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH,
                            },
                        )],
                    );
                }
                record["applicationId"] = json!(application_id);
            }
            if let Some(v) = resolved_bool_field(android, "appLinksEnabled") {
                record["appLinksEnabled"] = json!(v);
            }
            let fingerprints = resolved_string_list_field(android, "sha256CertFingerprints");
            if fingerprints.is_empty() {
                return mobile_app_payload(
                    &field.selection,
                    None,
                    vec![presence_user_error(
                        ["input", "android", "sha256CertFingerprints"],
                        "Sha256 cert fingerprints",
                    )],
                );
            }
            record["sha256CertFingerprints"] = json!(fingerprints);
        }
        if let Some(apple) = apple {
            if let Some(app_id) = resolved_string_field(apple, "appId") {
                if app_id.trim().is_empty() {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![presence_user_error(
                            ["mobilePlatformApplication", "apple", "appId"],
                            "App ID",
                        )],
                    );
                }
                if app_id.len() > MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH {
                    return mobile_app_payload(
                        &field.selection,
                        None,
                        vec![length_user_error(
                            ["input", "apple", "appId"],
                            "Application ID",
                            LengthUserErrorBound::TooLong {
                                maximum: MOBILE_PLATFORM_APPLICATION_ID_MAX_LENGTH,
                            },
                        )],
                    );
                }
                record["appId"] = json!(app_id);
            }
            if let Some(error) = validate_mobile_app_clip_application_id(apple, true) {
                return mobile_app_payload(&field.selection, None, vec![error]);
            }
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
        self.store
            .staged
            .online_store_integrations
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
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        script_tag_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn script_tag_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                vec![user_error(
                    ["displayScope"],
                    "Display scope is not included in the list",
                    Some("INCLUSION"),
                )],
            );
        }
        let Some(mut record) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .filter(|record| is_online_store_script_tag_record(record))
            .cloned()
        else {
            return script_tag_payload(
                &field.selection,
                None,
                vec![user_error_typed(
                    "ScriptTagUserError",
                    ["id"],
                    "Script tag not found",
                    Some("NOT_FOUND"),
                )],
            );
        };
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
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        script_tag_payload(&field.selection, Some(record), Vec::new())
    }

    pub(in crate::proxy) fn script_tag_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let is_staged_script_tag = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .is_some_and(is_online_store_script_tag_record);
        if !is_staged_script_tag {
            return deleted_script_tag_payload(
                &field.selection,
                Value::Null,
                vec![user_error_typed(
                    "ScriptTagUserError",
                    ["id"],
                    "Script tag not found",
                    Some("NOT_FOUND"),
                )],
            );
        }
        self.store.staged.online_store_integrations.remove(&id);
        staged_ids.push(id.clone());
        deleted_script_tag_payload(&field.selection, json!(id), Vec::new())
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
            "name": resolved_string_field(&field.arguments, "name").unwrap_or_else(|| "Local preview theme".to_string()),
            "role": resolved_string_field(&field.arguments, "role").unwrap_or_else(|| "UNPUBLISHED".to_string()),
            "processing": false,
            "processingFailed": false,
            "files": {"nodes": []}
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        theme_payload(&field.selection, record, Vec::new())
    }

    pub(in crate::proxy) fn theme_publish(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return theme_payload(
                &field.selection,
                Value::Null,
                vec![user_error_omit_code(
                    vec!["id"],
                    "Theme not found",
                    Some("NOT_FOUND"),
                )],
            );
        };
        let role = existing
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("UNPUBLISHED");
        if role == "DEVELOPMENT" {
            return theme_payload(
                &field.selection,
                Value::Null,
                vec![user_error(
                    ["base"],
                    "You cannot publish a development theme.",
                    None,
                )],
            );
        }
        if matches!(role, "DEMO" | "LOCKED" | "ARCHIVED") {
            return theme_payload(
                &field.selection,
                Value::Null,
                vec![user_error_omit_code(
                    ["id"],
                    &format!("Theme cannot be published from role {role}"),
                    None,
                )],
            );
        }
        for record in self.store.staged.online_store_integrations.values_mut() {
            if is_online_store_theme_record(record)
                && record.get("role").and_then(Value::as_str) == Some("MAIN")
            {
                record["role"] = json!("UNPUBLISHED");
            }
        }
        let mut theme = existing;
        theme["role"] = json!("MAIN");
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), theme.clone());
        staged_ids.push(id);
        theme_payload(&field.selection, theme, Vec::new())
    }

    pub(in crate::proxy) fn theme_update(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(mut theme) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return theme_payload(
                &field.selection,
                Value::Null,
                vec![user_error_omit_code(
                    vec!["id"],
                    "Theme not found",
                    Some("NOT_FOUND"),
                )],
            );
        };
        if theme.get("role").and_then(Value::as_str) == Some("LOCKED") {
            return theme_payload(
                &field.selection,
                Value::Null,
                vec![user_error_omit_code(
                    vec!["id"],
                    "Locked themes cannot be modified.",
                    Some("CANNOT_UPDATE_LOCKED_THEME"),
                )],
            );
        }
        let input = match field.arguments.get("input") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return theme_payload(&field.selection, theme, Vec::new()),
        };
        if let Some(name) = resolved_string_field(input, "name") {
            if name.trim().is_empty() {
                return theme_payload(
                    &field.selection,
                    Value::Null,
                    vec![user_error_omit_code(
                        vec!["input", "name"],
                        "Name can't be blank",
                        Some("INVALID"),
                    )],
                );
            }
            theme["name"] = json!(name);
        }
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), theme.clone());
        staged_ids.push(id);
        theme_payload(&field.selection, theme, Vec::new())
    }

    pub(in crate::proxy) fn theme_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(theme) = self
            .store
            .staged
            .online_store_integrations
            .get(&id)
            .cloned()
        else {
            return deleted_theme_payload(
                &field.selection,
                Value::Null,
                vec![user_error_omit_code(
                    vec!["id"],
                    "Theme not found",
                    Some("NOT_FOUND"),
                )],
            );
        };
        let main_count = self
            .store
            .staged
            .online_store_integrations
            .values()
            .filter(|record| {
                is_online_store_theme_record(record)
                    && record.get("role").and_then(Value::as_str) == Some("MAIN")
            })
            .count();
        if theme.get("role").and_then(Value::as_str) == Some("MAIN") && main_count <= 1 {
            return deleted_theme_payload(
                &field.selection,
                Value::Null,
                vec![user_error_omit_code(
                    vec!["id"],
                    "You can't delete your only published theme.",
                    Some("INVALID"),
                )],
            );
        }
        self.store.staged.online_store_integrations.remove(&id);
        staged_ids.push(id.clone());
        deleted_theme_payload(&field.selection, json!(id), Vec::new())
    }

    pub(in crate::proxy) fn theme_files_upsert(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let theme_id = resolved_string_field(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        if files.len() > THEME_FILES_MAX_FILE_INPUT {
            let payload = json!({
                "job": Value::Null,
                "upsertedThemeFiles": [],
                "userErrors": [theme_file_limit_error()]
            });
            return selected_json(&payload, &field.selection);
        }
        let mut errors = Vec::new();
        let mut seen_filenames = BTreeSet::new();
        for (index, file) in files.iter().enumerate() {
            let filename = theme_file_arg_string(file, "filename").unwrap_or_default();
            if let Some(error) = theme_file_filename_error(index, &filename) {
                errors.push(error);
            } else if !seen_filenames.insert(filename.clone()) {
                errors.push(theme_file_duplicate_error(index, "filename"));
            }
            if theme_file_record_from_input(file).is_err() {
                errors.push(theme_file_field_error(
                    index,
                    "body",
                    "invalid-body-input",
                    "INVALID",
                ));
            }
            if let Some(expected_checksum) = theme_file_arg_string(file, "checksumMd5") {
                if self
                    .find_theme_file(&theme_id, &filename)
                    .and_then(|record| {
                        record
                            .get("checksumMd5")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .is_some_and(|current_checksum| current_checksum != expected_checksum)
                {
                    errors.push(theme_file_field_error(
                        index,
                        "checksumMd5",
                        "Checksum does not match",
                        "CONFLICT",
                    ));
                }
            }
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({"job": Value::Null, "upsertedThemeFiles": [], "userErrors": errors}),
                &field.selection,
            );
        }
        let job = if files.iter().any(theme_file_input_uses_url_body) {
            json!({
                "__typename": "Job",
                "id": self.next_proxy_synthetic_gid("Job"),
                "done": false,
                "query": Value::Null
            })
        } else {
            Value::Null
        };
        let mut upserted = Vec::new();
        let mut staged = false;
        for file in files {
            if let Ok(Some(record)) = theme_file_record_from_input(&file) {
                let persisted = self.upsert_theme_file(&theme_id, record.clone());
                staged |= persisted.is_some();
                let record = persisted.unwrap_or(record);
                upserted.push(theme_file_operation_result(&record));
            }
        }
        if staged {
            staged_ids.push(theme_id);
        }
        selected_json(
            &json!({"job": job, "upsertedThemeFiles": upserted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_copy(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let theme_id = resolved_string_field(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_list_arg(&field.arguments, "files");
        if files.len() > THEME_FILES_MAX_FILE_INPUT {
            return selected_json(
                &json!({"copiedThemeFiles": [], "userErrors": [theme_file_limit_error()]}),
                &field.selection,
            );
        }
        let mut preflight_errors = Vec::new();
        let mut seen_dst_filenames = BTreeSet::new();
        for (index, file) in files.iter().enumerate() {
            let dst = theme_file_arg_string(file, "dstFilename").unwrap_or_default();
            if !dst.is_empty() && !seen_dst_filenames.insert(dst) {
                preflight_errors.push(theme_file_duplicate_error(index, "dstFilename"));
            }
        }
        if !preflight_errors.is_empty() {
            return selected_json(
                &json!({"copiedThemeFiles": [], "userErrors": preflight_errors}),
                &field.selection,
            );
        }
        let mut copied = Vec::new();
        let mut errors = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let src = theme_file_arg_string(file, "srcFilename").unwrap_or_default();
            let dst = theme_file_arg_string(file, "dstFilename").unwrap_or_default();
            let Some(source_file) = self.find_theme_file(&theme_id, &src) else {
                errors.push(user_error(
                    vec![
                        "files".to_string(),
                        index.to_string(),
                        "srcFilename".to_string(),
                    ],
                    "File not found",
                    Some("NOT_FOUND"),
                ));
                continue;
            };
            let content = source_file["body"]["content"].as_str().unwrap_or_default();
            let record = theme_file_record(&dst, content);
            copied.push(record);
        }
        let copied_results = copied
            .iter()
            .filter_map(|file| self.upsert_theme_file(&theme_id, file.clone()))
            .map(|file| theme_file_operation_result(&file))
            .collect::<Vec<_>>();
        if !copied_results.is_empty() {
            staged_ids.push(theme_id);
        }
        selected_json(
            &json!({"copiedThemeFiles": copied_results, "userErrors": errors}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn theme_files_delete(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let theme_id = resolved_string_field(&field.arguments, "themeId").unwrap_or_default();
        let files = resolved_string_list_arg(&field.arguments, "files");
        if files.len() > THEME_FILES_MAX_FILE_LIMIT {
            return selected_json(
                &json!({"deletedThemeFiles": [], "userErrors": [theme_file_limit_error()]}),
                &field.selection,
            );
        }
        let mut errors = Vec::new();
        let mut seen_filenames = BTreeSet::new();
        for (index, filename) in files.iter().enumerate() {
            if !seen_filenames.insert(filename.clone()) {
                errors.push(theme_file_delete_error(
                    index,
                    "duplicate-file-input",
                    "INVALID",
                ));
            }
            if THEME_UNDELETABLE_FILES.contains(&filename.as_str()) {
                errors.push(theme_file_delete_error(
                    index,
                    "File is required and can't be deleted",
                    "INVALID",
                ));
            }
        }
        if !errors.is_empty() {
            return selected_json(
                &json!({"deletedThemeFiles": [], "userErrors": errors}),
                &field.selection,
            );
        }
        let mut deleted = Vec::new();
        if let Some(theme) = self
            .store
            .staged
            .online_store_integrations
            .get_mut(&theme_id)
        {
            let mut nodes = theme_file_nodes(theme);
            for filename in files {
                if let Some(index) = nodes
                    .iter()
                    .position(|file| file["filename"].as_str() == Some(filename.as_str()))
                {
                    let removed = nodes.remove(index);
                    deleted.push(theme_file_operation_result(&removed));
                }
            }
            set_theme_file_nodes(theme, nodes);
        }
        if !deleted.is_empty() {
            staged_ids.push(theme_id);
        }
        selected_json(
            &json!({"deletedThemeFiles": deleted, "userErrors": []}),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn upsert_theme_file(
        &mut self,
        theme_id: &str,
        mut file: Value,
    ) -> Option<Value> {
        let theme = self
            .store
            .staged
            .online_store_integrations
            .get_mut(theme_id)?;
        let timestamp = online_store_operation_timestamp();
        let filename = file["filename"].as_str().unwrap_or_default().to_string();
        let mut nodes = theme_file_nodes(theme);
        let persisted = if let Some(index) = nodes
            .iter()
            .position(|existing| existing["filename"].as_str() == Some(filename.as_str()))
        {
            if let Some(created_at) = nodes[index].get("createdAt").cloned() {
                file["createdAt"] = created_at;
            }
            file["updatedAt"] = json!(timestamp);
            nodes[index] = file;
            nodes[index].clone()
        } else {
            file["createdAt"] = json!(timestamp.clone());
            file["updatedAt"] = json!(timestamp);
            nodes.push(file);
            nodes.last().cloned().unwrap_or(Value::Null)
        };
        set_theme_file_nodes(theme, nodes);
        Some(persisted)
    }

    pub(in crate::proxy) fn find_theme_file(
        &self,
        theme_id: &str,
        filename: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .online_store_integrations
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
            .store
            .staged
            .online_store_integrations
            .values()
            .any(is_web_pixel_record)
        {
            return web_pixel_payload(
                &field.selection,
                Value::Null,
                vec![user_error_typed(
                    "WebPixelUserError",
                    Value::Null,
                    "Web pixel is taken.",
                    Some("TAKEN"),
                )],
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
            .and_then(web_pixel_settings_from_resolved)
            .unwrap_or_else(|| json!({}));
        let record = json!({
            "__typename": "WebPixel",
            "id": id,
            "settings": settings
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        web_pixel_payload(&field.selection, record, Vec::new())
    }

    pub(in crate::proxy) fn web_pixel_update(
        &mut self,
        field: &RootFieldSelection,
        allow_missing_upsert: bool,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !allow_missing_upsert
            && !self
                .store
                .staged
                .online_store_integrations
                .get(&id)
                .is_some_and(is_web_pixel_record)
        {
            return web_pixel_payload(
                &field.selection,
                Value::Null,
                vec![user_error_typed(
                    "WebPixelUserError",
                    ["id"],
                    "Pixel not found",
                    Some("NOT_FOUND"),
                )],
            );
        }
        let input = match field.arguments.get("webPixel") {
            Some(ResolvedValue::Object(input)) => input,
            _ => return web_pixel_payload(&field.selection, Value::Null, Vec::new()),
        };
        let settings_raw = resolved_string_field(input, "settings").unwrap_or_default();
        let Ok(settings) = serde_json::from_str::<Value>(&settings_raw) else {
            return web_pixel_payload(
                &field.selection,
                Value::Null,
                vec![user_error_typed(
                    "WebPixelUserError",
                    ["settings"],
                    "Settings must be valid JSON",
                    Some("INVALID_CONFIGURATION_JSON"),
                )],
            );
        };
        let record = json!({
            "__typename": "WebPixel",
            "id": id,
            "settings": settings
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        web_pixel_payload(&field.selection, record, Vec::new())
    }

    pub(in crate::proxy) fn server_pixel_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = self.next_online_store_id("ServerPixel");
        let record = json!({
            "__typename": "ServerPixel",
            "id": id,
            "status": server_pixel_status_for_endpoint(None),
            "webhookEndpointAddress": null
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        server_pixel_payload(&field.selection, record, Vec::new())
    }

    pub(in crate::proxy) fn server_pixel_endpoint_update(
        &mut self,
        field: &RootFieldSelection,
        kind: &str,
    ) -> Value {
        let Some(id) = self
            .store
            .staged
            .online_store_integrations
            .iter()
            .find(|(_, v)| is_server_pixel_record(v))
            .map(|(id, _)| id.clone())
        else {
            return server_pixel_payload(
                &field.selection,
                Value::Null,
                vec![user_error_typed(
                    "ServerPixelUserError",
                    ["id"],
                    "Server pixel not found",
                    Some("NOT_FOUND"),
                )],
            );
        };
        let endpoint = if kind == "arn" {
            let arn = resolved_string_field(&field.arguments, "arn").unwrap_or_default();
            if !arn.starts_with("arn:aws:events:") || arn.trim().is_empty() {
                return server_pixel_payload(
                    &field.selection,
                    Value::Null,
                    vec![user_error_typed(
                        "ServerPixelUserError",
                        ["arn"],
                        &format!("Invalid ARN '{arn}'"),
                        Some("INVALID_FIELD_ARGUMENTS"),
                    )],
                );
            }
            arn
        } else {
            let project =
                resolved_string_field(&field.arguments, "pubSubProject").unwrap_or_default();
            let topic = resolved_string_field(&field.arguments, "pubSubTopic").unwrap_or_default();
            let mut errors = Vec::new();
            if project.trim().is_empty() {
                errors.push(user_error_typed(
                    "ServerPixelUserError",
                    ["pubSubProject"],
                    "pubSubProject can't be blank",
                    Some("INVALID_FIELD_ARGUMENTS"),
                ));
            }
            if topic.trim().is_empty() {
                errors.push(user_error_typed(
                    "ServerPixelUserError",
                    ["pubSubTopic"],
                    "pubSubTopic can't be blank",
                    Some("INVALID_FIELD_ARGUMENTS"),
                ));
            }
            if !errors.is_empty() {
                return server_pixel_payload(&field.selection, Value::Null, errors);
            }
            format!("{project}/{topic}")
        };
        let record = json!({
            "__typename": "ServerPixel",
            "id": id,
            "status": server_pixel_status_for_endpoint(Some(&endpoint)),
            "webhookEndpointAddress": endpoint
        });
        self.store
            .staged
            .online_store_integrations
            .insert(id, record.clone());
        server_pixel_payload(&field.selection, record, Vec::new())
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
            return storefront_access_token_payload(
                &field.selection,
                Value::Null,
                self.store.effective_shop(),
                vec![presence_user_error(["input", "title"], "Title")],
            );
        }
        let token_count = self
            .store
            .staged
            .online_store_integrations
            .values()
            .filter(|record| is_storefront_access_token_record(record))
            .count();
        if token_count >= 100 {
            return storefront_access_token_payload(
                &field.selection,
                Value::Null,
                self.store.effective_shop(),
                vec![user_error(
                    ["input"],
                    "apps.admin.graph_api_errors.storefront_access_token_create.reached_limit",
                    Some("REACHED_LIMIT"),
                )],
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
        self.store
            .staged
            .online_store_integrations
            .insert(id.clone(), record.clone());
        staged_ids.push(id);
        storefront_access_token_payload(
            &field.selection,
            record,
            self.store.effective_shop(),
            Vec::new(),
        )
    }
}

fn is_online_store_sales_channel_query_root(root: &str) -> bool {
    matches!(
        root,
        "mobilePlatformApplication"
            | "mobilePlatformApplications"
            | "scriptTag"
            | "scriptTags"
            | "serverPixel"
            | "theme"
            | "themes"
            | "urlRedirect"
            | "urlRedirects"
            | "urlRedirectsCount"
            | "webPixel"
    )
}

fn observed_sales_channel_record(record: &Value) -> Option<(String, Value)> {
    let id = record.get("id").and_then(Value::as_str)?.to_string();
    let typename = record
        .get("__typename")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| shopify_gid_resource_type(&id).map(str::to_string))?;
    let mut record = record.clone();
    match typename.as_str() {
        "OnlineStoreTheme" => {
            record["__typename"] = json!("OnlineStoreTheme");
            if record.get("files").is_none() {
                record["files"] = json!({"nodes": []});
            }
        }
        "ScriptTag" => {}
        "WebPixel" => {
            record["__typename"] = json!("WebPixel");
        }
        "ServerPixel" => {
            record["__typename"] = json!("ServerPixel");
        }
        "AppleApplication" | "AndroidApplication" => {
            record["__typename"] = json!(typename);
        }
        "MobilePlatformApplication" => {
            if record.get("__typename").is_none() {
                record["__typename"] = json!("MobilePlatformApplication");
            }
        }
        "StorefrontAccessToken" => {
            record["__typename"] = json!("StorefrontAccessToken");
        }
        _ => return None,
    }
    Some((id, record))
}

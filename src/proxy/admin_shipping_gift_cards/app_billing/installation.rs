use super::*;

// Runtime messages mirror Core i18n keys under
// apps.admin.graph_api_errors.app_uninstall; the add_error_code placeholders
// use different text.
const APP_UNINSTALL_APP_NOT_FOUND_MESSAGE: &str = "App not found";
const APP_UNINSTALL_APP_NOT_INSTALLED_MESSAGE: &str = "App is not installed on shop";

impl DraftProxy {
    pub(in crate::proxy) fn observe_current_app_installation_data(
        &mut self,
        request: &Request,
        data: &Value,
    ) {
        let Some(observed) = data.get("currentAppInstallation") else {
            return;
        };
        if !observed.is_object() {
            return;
        }
        self.observe_app_installation(observed);
        if let Some(app_id) = app_id_from_installation(observed) {
            self.remember_current_app_request_context(request, &app_id);
        }
    }

    pub(in crate::proxy) fn rebuild_app_graph_indexes(&mut self) {
        self.store.base.app_installation_ids_by_app_id.clear();
        self.store.base.app_ids_by_handle.clear();
        self.store.base.app_ids_by_api_key.clear();
        for (app_id, app) in &self.store.base.apps.records {
            if let Some(handle) = app.get("handle").and_then(Value::as_str) {
                self.store
                    .base
                    .app_ids_by_handle
                    .insert(handle.to_string(), app_id.clone());
            }
            if let Some(api_key) = app.get("apiKey").and_then(Value::as_str) {
                self.store
                    .base
                    .app_ids_by_api_key
                    .insert(api_key.to_string(), app_id.clone());
            }
        }
        for (installation_id, installation) in &self.store.base.app_installations.records {
            if let Some(app_id) = app_id_from_installation(installation) {
                self.store
                    .base
                    .app_installation_ids_by_app_id
                    .insert(app_id, installation_id.clone());
            }
        }
    }

    fn remember_current_app_request_context(&mut self, request: &Request, app_id: &str) {
        self.store
            .base
            .current_app_ids_by_request_context
            .insert(request_app_context_key(request), app_id.to_string());
        self.store
            .base
            .current_app_ids_by_request_context
            .insert(request_app_gid(request), app_id.to_string());
    }

    fn observe_app(&mut self, observed: &Value) {
        let Some(app_id) = observed
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let base = self
            .store
            .base
            .apps
            .get(&app_id)
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mut merged = merge_app_installation_json(&base, observed);
        if let Some(installation) = observed
            .get("installation")
            .filter(|value| value.is_object())
        {
            let mut installation = installation.clone();
            let mut nested_app = merged.clone();
            if let Some(fields) = nested_app.as_object_mut() {
                fields.remove("installation");
            }
            installation["app"] = nested_app;
            self.observe_app_installation(&installation);
            if let Some(observed_installation) = self.effective_app_installation_for_app(&app_id) {
                merged["installation"] = observed_installation;
            }
        }
        self.store.base.apps.insert(app_id.clone(), merged.clone());
        if let Some(handle) = merged.get("handle").and_then(Value::as_str) {
            self.store
                .base
                .app_ids_by_handle
                .insert(handle.to_string(), app_id.clone());
        }
        if let Some(api_key) = merged.get("apiKey").and_then(Value::as_str) {
            self.store
                .base
                .app_ids_by_api_key
                .insert(api_key.to_string(), app_id);
        }
    }

    fn observe_app_installation(&mut self, observed: &Value) {
        let Some(installation_id) = app_installation_id(observed) else {
            return;
        };
        let Some(app_id) = app_id_from_installation(observed) else {
            return;
        };
        if let Some(app) = observed.get("app").filter(|value| value.is_object()) {
            let mut app = app.clone();
            if let Some(fields) = app.as_object_mut() {
                fields.remove("installation");
            }
            self.observe_app(&app);
        }
        let base = self
            .store
            .base
            .app_installations
            .get(&installation_id)
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mut merged = merge_app_installation_json(&base, observed);
        if observed.get("accessScopes").is_some()
            && merged.get("__draftProxySource").and_then(Value::as_str)
                == Some("observed-identity-only")
        {
            if let Some(fields) = merged.as_object_mut() {
                fields.remove("__draftProxySource");
            }
        }
        if let Some(app) = self.store.base.apps.get(&app_id) {
            merged["app"] = app.clone();
        }
        self.store
            .base
            .app_installations
            .insert(installation_id.clone(), merged);
        self.store
            .base
            .app_installation_ids_by_app_id
            .insert(app_id, installation_id);
    }

    pub(super) fn observe_app_query_data(&mut self, invocation: &RootInvocation<'_>, data: &Value) {
        for root in &invocation.operation_roots {
            let Some(value) = data.get(&root.response_key) else {
                continue;
            };
            match root.name.as_str() {
                "app" | "appByHandle" | "appByKey" if value.is_object() => {
                    self.observe_app(value);
                    if root.name == "app" && !root.arguments.contains_key("id") {
                        if let Some(app_id) = value.get("id").and_then(Value::as_str) {
                            self.remember_current_app_request_context(invocation.request, app_id);
                        }
                    }
                }
                "appInstallation" | "currentAppInstallation" if value.is_object() => {
                    let mut observed = value.clone();
                    if root.name == "currentAppInstallation"
                        && observed.get("accessScopes").is_none()
                    {
                        observed["__draftProxySource"] = json!("observed-identity-only");
                    }
                    self.observe_app_installation(&observed);
                    if root.name == "currentAppInstallation"
                        || (root.name == "appInstallation" && !root.arguments.contains_key("id"))
                    {
                        if let Some(app_id) = app_id_from_installation(value) {
                            self.remember_current_app_request_context(invocation.request, &app_id);
                        }
                    }
                }
                "appInstallations" if value.is_object() => {
                    self.observe_app_installation_connection(&root.arguments, value);
                }
                _ => {}
            }
        }
    }

    fn observe_app_installation_connection(
        &mut self,
        arguments: &BTreeMap<String, Value>,
        connection: &Value,
    ) {
        let mut rows = observed_connection_rows(connection);
        for row in &rows {
            self.observe_app_installation(&row.node);
        }
        let reverse = arguments
            .get("reverse")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let complete = arguments.get("after").is_none_or(Value::is_null)
            && arguments.get("before").is_none_or(Value::is_null)
            && !connection
                .pointer("/pageInfo/hasNextPage")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && !connection
                .pointer("/pageInfo/hasPreviousPage")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let window = app_installation_catalog_window(&rows, connection);
        let scope_key = app_installation_catalog_scope_key(arguments);
        let window_key = app_installation_catalog_window_key(arguments);
        if reverse {
            rows.reverse();
        }
        let scope = self
            .store
            .base
            .app_installation_catalog_scopes
            .entry(scope_key)
            .or_default();
        if complete {
            scope.installation_ids.clear();
            scope.cursors.clear();
        }
        for row in rows {
            let Some(id) = app_installation_id(&row.node) else {
                continue;
            };
            if !scope.installation_ids.contains(&id) {
                scope.installation_ids.push(id.clone());
            }
            if let Some(cursor) = row.cursor {
                scope.cursors.insert(id, cursor);
            }
        }
        scope.complete |= complete;
        scope.windows.insert(window_key, window);
    }

    pub(super) fn ensure_current_app_installation(&mut self, request: &Request) -> String {
        let app_id = request_app_gid(request);
        if let Some(observed_app_id) = self.current_app_installation_app_id_for_request(&app_id) {
            return observed_app_id;
        }
        self.store
            .staged
            .installed_apps
            .entry(app_id.clone())
            .or_insert_with(|| current_app_installation_from_request(request));
        app_id
    }

    pub(in crate::proxy) fn current_app_installation_app_id_for_request(
        &self,
        request_app_id: &str,
    ) -> Option<String> {
        if self
            .store
            .staged
            .installed_apps
            .contains_key(request_app_id)
        {
            return Some(request_app_id.to_string());
        }
        if self
            .store
            .base
            .app_installation_ids_by_app_id
            .contains_key(request_app_id)
        {
            return Some(request_app_id.to_string());
        }
        self.store
            .staged
            .installed_apps
            .iter()
            .find_map(|(app_id, installation)| {
                (request_app_id_from_installation(installation).as_deref() == Some(request_app_id))
                    .then(|| app_id.clone())
            })
            .or_else(|| {
                self.store
                    .base
                    .current_app_ids_by_request_context
                    .get(request_app_id)
                    .cloned()
            })
    }

    pub(in crate::proxy) fn app_installation_for_app(&self, app_id: &str) -> Option<Value> {
        self.effective_app_installation_for_app(app_id)
    }

    pub(super) fn revoked_access_scopes_for_app(&self, app_id: &str) -> BTreeSet<String> {
        self.store
            .staged
            .revoked_app_access_scopes
            .get(app_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::proxy) fn current_app_installation_root_value(&self, request: &Request) -> Value {
        let request_app_id = request_app_gid(request);
        let app_id = self
            .current_app_installation_app_id_for_request(&request_app_id)
            .or_else(|| {
                self.store
                    .base
                    .current_app_ids_by_request_context
                    .get(&request_app_context_key(request))
                    .cloned()
            })
            .or_else(|| request_has_explicit_app_context(request).then(|| request_app_id.clone()));
        let Some(app_id) = app_id else {
            return Value::Null;
        };
        let installation = self
            .effective_app_installation_for_app(&app_id)
            .or_else(|| {
                request_has_explicit_app_context(request)
                    .then(|| current_app_installation_from_request(request))
            });
        if self.store.staged.uninstalled_app_ids.contains(&app_id) {
            return Value::Null;
        }
        installation
            .as_ref()
            .map(|installation| self.effective_app_installation_value(&app_id, installation))
            .unwrap_or(Value::Null)
    }

    pub(crate) fn app_identity_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let has_local_overlay = self.app_graph_has_local_overlay();
        if self.config.read_mode == ReadMode::LiveHybrid {
            let result = self.cached_or_forward_upstream_graphql_result(
                invocation.request,
                invocation.response_key,
            );
            if result.transport_succeeded {
                self.observe_app_query_data(&invocation, &result.data);
            }
            if !has_local_overlay || !result.outcome.errors.is_empty() {
                return result.outcome;
            }
        }
        ResolverOutcome::value(self.app_identity_root_value(&invocation))
    }

    fn app_identity_root_value(&self, invocation: &RootInvocation<'_>) -> Value {
        match invocation.root_name {
            "app" => match invocation.arguments.get("id").and_then(Value::as_str) {
                Some(id) => self.effective_app_value_by_id(id).unwrap_or_else(|| {
                    self.current_app_value(invocation.request)
                        .filter(|app| app.get("id").and_then(Value::as_str) == Some(id))
                        .unwrap_or(Value::Null)
                }),
                None => self
                    .current_app_value(invocation.request)
                    .unwrap_or(Value::Null),
            },
            "appByHandle" => invocation
                .arguments
                .get("handle")
                .and_then(Value::as_str)
                .and_then(|handle| self.effective_app_value_by_handle(handle))
                .unwrap_or(Value::Null),
            "appByKey" => invocation
                .arguments
                .get("apiKey")
                .and_then(Value::as_str)
                .and_then(|api_key| self.effective_app_value_by_api_key(api_key))
                .unwrap_or(Value::Null),
            "appInstallation" => match invocation.arguments.get("id").and_then(Value::as_str) {
                Some(id) => self
                    .effective_app_installation_value_by_id(id)
                    .unwrap_or_else(|| {
                        let current = self.current_app_installation_root_value(invocation.request);
                        if current.get("id").and_then(Value::as_str) == Some(id) {
                            current
                        } else {
                            Value::Null
                        }
                    }),
                None => self.current_app_installation_root_value(invocation.request),
            },
            "appInstallations" => self.app_installations_connection_value(&invocation.arguments),
            root => json!({ "unsupportedAppIdentityRoot": root }),
        }
    }

    pub(super) fn app_graph_has_local_overlay(&self) -> bool {
        !self.store.staged.installed_apps.is_empty()
            || !self.store.staged.revoked_app_access_scopes.is_empty()
            || !self.store.staged.uninstalled_app_ids.is_empty()
            || !self.store.staged.app_subscriptions.is_empty()
            || !self.store.staged.app_one_time_purchases.is_empty()
    }

    fn effective_app_installation_for_app(&self, app_id: &str) -> Option<Value> {
        let base = self
            .store
            .base
            .app_installation_ids_by_app_id
            .get(app_id)
            .and_then(|id| self.store.base.app_installations.get(id))
            .cloned();
        self.store
            .staged
            .installed_apps
            .get(app_id)
            .map(|staged| {
                base.as_ref()
                    .map(|base| merge_app_installation_json(base, staged))
                    .unwrap_or_else(|| staged.clone())
            })
            .or(base)
    }

    fn effective_app_installation_value(&self, app_id: &str, installation: &Value) -> Value {
        let subscriptions = self.app_subscriptions_for_app(app_id);
        let purchases = self.app_one_time_purchases_for_app(app_id);
        current_app_installation_value(
            installation,
            &subscriptions,
            &purchases,
            &self.revoked_access_scopes_for_app(app_id),
        )
    }

    pub(in crate::proxy) fn effective_app_installation_value_by_id(
        &self,
        installation_id: &str,
    ) -> Option<Value> {
        let app_id = self
            .store
            .base
            .app_installations
            .get(installation_id)
            .and_then(app_id_from_installation)
            .or_else(|| {
                self.store
                    .staged
                    .installed_apps
                    .iter()
                    .find_map(|(app_id, installation)| {
                        (app_installation_id(installation).as_deref() == Some(installation_id))
                            .then(|| app_id.clone())
                    })
            })?;
        if self.store.staged.uninstalled_app_ids.contains(&app_id) {
            return None;
        }
        self.effective_app_installation_for_app(&app_id)
            .map(|installation| self.effective_app_installation_value(&app_id, &installation))
    }

    pub(in crate::proxy) fn effective_app_value_by_id(&self, app_id: &str) -> Option<Value> {
        let base = self.store.base.apps.get(app_id).cloned();
        let staged = self
            .store
            .staged
            .installed_apps
            .get(app_id)
            .and_then(|installation| installation.get("app"))
            .cloned();
        let mut app = match (base, staged) {
            (Some(base), Some(staged)) => merge_app_installation_json(&base, &staged),
            (Some(base), None) => base,
            (None, Some(staged)) => staged,
            (None, None) => return None,
        };
        app["__typename"] = json!("App");
        app["installation"] = if self.store.staged.uninstalled_app_ids.contains(app_id) {
            Value::Null
        } else {
            self.effective_app_installation_for_app(app_id)
                .map(|installation| self.effective_app_installation_value(app_id, &installation))
                .unwrap_or(Value::Null)
        };
        Some(app)
    }

    fn effective_app_value_by_handle(&self, handle: &str) -> Option<Value> {
        self.store
            .staged
            .installed_apps
            .iter()
            .find_map(|(app_id, installation)| {
                (installation.pointer("/app/handle").and_then(Value::as_str) == Some(handle))
                    .then(|| app_id.clone())
            })
            .or_else(|| self.store.base.app_ids_by_handle.get(handle).cloned())
            .and_then(|app_id| self.effective_app_value_by_id(&app_id))
    }

    fn effective_app_value_by_api_key(&self, api_key: &str) -> Option<Value> {
        self.store
            .staged
            .installed_apps
            .iter()
            .find_map(|(app_id, installation)| {
                (installation.pointer("/app/apiKey").and_then(Value::as_str) == Some(api_key))
                    .then(|| app_id.clone())
            })
            .or_else(|| self.store.base.app_ids_by_api_key.get(api_key).cloned())
            .and_then(|app_id| self.effective_app_value_by_id(&app_id))
    }

    fn current_app_value(&self, request: &Request) -> Option<Value> {
        let request_app_id = request_app_gid(request);
        let app_id = self
            .current_app_installation_app_id_for_request(&request_app_id)
            .or_else(|| {
                self.store
                    .base
                    .current_app_ids_by_request_context
                    .get(&request_app_context_key(request))
                    .cloned()
            })
            .or_else(|| request_has_explicit_app_context(request).then_some(request_app_id))?;
        self.effective_app_value_by_id(&app_id).or_else(|| {
            request_has_explicit_app_context(request)
                .then(|| current_app_installation_from_request(request)["app"].clone())
                .map(|mut app| {
                    app["installation"] = current_app_installation_from_request(request);
                    app
                })
        })
    }

    fn app_subscriptions_for_app(&self, app_id: &str) -> BTreeMap<String, Value> {
        self.store
            .staged
            .app_subscriptions
            .iter()
            .filter(|(_, record)| {
                record
                    .get("__draftProxyAppId")
                    .and_then(Value::as_str)
                    .is_none_or(|owner| owner == app_id)
            })
            .map(|(id, record)| (id.clone(), record.clone()))
            .collect()
    }

    fn app_one_time_purchases_for_app(&self, app_id: &str) -> BTreeMap<String, Value> {
        self.store
            .staged
            .app_one_time_purchases
            .iter()
            .filter(|(_, record)| {
                record
                    .get("__draftProxyAppId")
                    .and_then(Value::as_str)
                    .is_none_or(|owner| owner == app_id)
            })
            .map(|(id, record)| (id.clone(), record.clone()))
            .collect()
    }

    fn app_installations_connection_value(&self, arguments: &BTreeMap<String, Value>) -> Value {
        let scope_key = app_installation_catalog_scope_key(arguments);
        let Some(scope) = self
            .store
            .base
            .app_installation_catalog_scopes
            .get(&scope_key)
        else {
            return connection_json(Vec::new());
        };
        let resolved_arguments = resolved_arguments_from_json(arguments);
        if scope.complete {
            let mut rows = scope
                .installation_ids
                .iter()
                .filter_map(|id| {
                    self.effective_app_installation_value_by_id(id)
                        .map(|installation| {
                            (
                                installation,
                                scope.cursors.get(id).cloned().unwrap_or_else(|| id.clone()),
                            )
                        })
                })
                .collect::<Vec<_>>();
            if arguments
                .get("reverse")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                rows.reverse();
            }
            let (rows, page_info) =
                connection_window(&rows, &resolved_arguments, |row| row.1.clone());
            let nodes = rows.iter().map(|row| row.0.clone()).collect::<Vec<_>>();
            return connection_json_with_cursor(nodes, |index, _| rows[index].1.clone(), page_info);
        }
        let Some(window) = scope
            .windows
            .get(&app_installation_catalog_window_key(arguments))
        else {
            return connection_json(Vec::new());
        };
        let rows = window
            .installation_ids
            .iter()
            .filter_map(|id| {
                self.effective_app_installation_value_by_id(id)
                    .map(|installation| {
                        (
                            installation,
                            window
                                .cursors
                                .get(id)
                                .cloned()
                                .unwrap_or_else(|| id.clone()),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let nodes = rows.iter().map(|row| row.0.clone()).collect::<Vec<_>>();
        let page_info = connection_page_info(
            window
                .page_info
                .get("hasNextPage")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            window
                .page_info
                .get("hasPreviousPage")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rows.first().map(|row| row.1.clone()),
            rows.last().map(|row| row.1.clone()),
        );
        connection_json_with_cursor(nodes, |index, _| rows[index].1.clone(), page_info)
    }

    pub(in crate::proxy) fn find_staged_app_usage_record(&self, id: &str) -> Option<Value> {
        self.store
            .staged
            .app_subscriptions
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

    pub(crate) fn app_uninstall(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let requested_id = resolved_object_field(&arguments, "input")
            .and_then(|input| resolved_string_field(&input, "id"));

        let current_app_id = self.ensure_current_app_installation(invocation.request);
        let target_app_id = requested_id
            .as_deref()
            .map(normalize_app_gid)
            .unwrap_or_else(|| current_app_id.clone());

        let (app, user_errors) = match self.app_installation_for_app(&target_app_id) {
            Some(_)
                if self
                    .store
                    .staged
                    .uninstalled_app_ids
                    .contains(&target_app_id) =>
            {
                (
                    Value::Null,
                    vec![user_error(
                        ["id"],
                        APP_UNINSTALL_APP_NOT_INSTALLED_MESSAGE,
                        Some("APP_NOT_INSTALLED"),
                    )],
                )
            }
            None => (
                Value::Null,
                vec![user_error(
                    ["id"],
                    APP_UNINSTALL_APP_NOT_FOUND_MESSAGE,
                    Some("APP_NOT_FOUND"),
                )],
            ),
            Some(installation) => {
                self.store
                    .staged
                    .uninstalled_app_ids
                    .insert(target_app_id.clone());
                for subscription in self.store.staged.app_subscriptions.values_mut() {
                    if subscription
                        .get("__draftProxyAppId")
                        .and_then(Value::as_str)
                        .is_some_and(|owner| owner != target_app_id)
                    {
                        continue;
                    }
                    if let Value::Object(fields) = subscription {
                        fields.insert("status".to_string(), json!("CANCELLED"));
                    }
                }
                self.store
                    .staged
                    .delegate_access_tokens
                    .retain(|_, record| {
                        record
                            .get("apiClientId")
                            .and_then(Value::as_str)
                            .map(normalize_app_gid)
                            .is_none_or(|api_client_id| api_client_id != target_app_id)
                    });
                self.record_mutation_log_entry(
                    invocation.request,
                    invocation.query,
                    invocation.variables,
                    "appUninstall",
                    vec![target_app_id.clone()],
                );
                (
                    installation.get("app").cloned().unwrap_or(Value::Null),
                    vec![],
                )
            }
        };
        ResolverOutcome::value(json!({
            "app": app,
            "userErrors": user_errors,
        }))
    }
}

fn app_installation_catalog_scope_key(arguments: &BTreeMap<String, Value>) -> String {
    format!(
        "category={}|privacy={}|sortKey={}",
        app_installation_argument_token(arguments, "category", "ALL"),
        app_installation_argument_token(arguments, "privacy", "PUBLIC"),
        app_installation_argument_token(arguments, "sortKey", "INSTALLED_AT")
    )
}

fn app_installation_catalog_window_key(arguments: &BTreeMap<String, Value>) -> String {
    format!(
        "first={}|after={}|last={}|before={}|reverse={}",
        app_installation_argument_token(arguments, "first", "null"),
        app_installation_argument_token(arguments, "after", "null"),
        app_installation_argument_token(arguments, "last", "null"),
        app_installation_argument_token(arguments, "before", "null"),
        app_installation_argument_token(arguments, "reverse", "false")
    )
}

fn app_installation_argument_token(
    arguments: &BTreeMap<String, Value>,
    name: &str,
    default: &str,
) -> String {
    arguments
        .get(name)
        .filter(|value| !value.is_null())
        .map(|value| match value {
            Value::String(value) => value.clone(),
            _ => value.to_string(),
        })
        .unwrap_or_else(|| default.to_string())
}

fn app_installation_catalog_window(
    rows: &[ObservedConnectionRow],
    connection: &Value,
) -> AppInstallationCatalogWindow {
    let mut window = AppInstallationCatalogWindow {
        page_info: connection
            .get("pageInfo")
            .cloned()
            .unwrap_or_else(empty_page_info),
        ..Default::default()
    };
    for row in rows {
        let Some(id) = app_installation_id(&row.node) else {
            continue;
        };
        window.installation_ids.push(id.clone());
        if let Some(cursor) = &row.cursor {
            window.cursors.insert(id, cursor.clone());
        }
    }
    window
}

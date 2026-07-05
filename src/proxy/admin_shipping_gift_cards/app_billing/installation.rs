use super::*;

// Runtime messages mirror Core i18n keys under
// apps.admin.graph_api_errors.app_uninstall; the add_error_code placeholders
// use different text.
const APP_UNINSTALL_APP_NOT_FOUND_MESSAGE: &str = "App not found";
const APP_UNINSTALL_APP_NOT_INSTALLED_MESSAGE: &str = "App is not installed on shop";

impl DraftProxy {
    pub(in crate::proxy) fn observe_current_app_installation_response(
        &mut self,
        request: &Request,
        response: &Response,
    ) {
        let Some(observed) = response.body.pointer("/data/currentAppInstallation") else {
            return;
        };
        if !observed.is_object() {
            return;
        }
        let request_record = current_app_installation_from_request(request);
        let request_app_id =
            app_id_from_installation(&request_record).unwrap_or_else(|| request_app_gid(request));
        let observed_app_id =
            app_id_from_installation(observed).unwrap_or_else(|| request_app_id.clone());
        let base = self
            .store
            .staged
            .installed_apps
            .get(&observed_app_id)
            .cloned()
            .unwrap_or(request_record);
        let merged = merge_app_installation_json(&base, observed);
        let app_id = app_id_from_installation(&merged).unwrap_or(observed_app_id);
        self.store.staged.installed_apps.insert(app_id, merged);
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
        self.store
            .staged
            .installed_apps
            .iter()
            .find_map(|(app_id, installation)| {
                (request_app_id_from_installation(installation).as_deref() == Some(request_app_id))
                    .then(|| app_id.clone())
            })
    }

    pub(super) fn app_installation_for_app(&self, app_id: &str) -> Option<&Value> {
        self.store.staged.installed_apps.get(app_id)
    }

    pub(super) fn revoked_access_scopes_for_app(&self, app_id: &str) -> BTreeSet<String> {
        self.store
            .staged
            .revoked_app_access_scopes
            .get(app_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::proxy) fn current_app_installation_read_data(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Value {
        let app_id = self.ensure_current_app_installation(request);
        let installation = self.app_installation_for_app(&app_id).cloned();
        let revoked_access_scopes = self.revoked_access_scopes_for_app(&app_id);
        root_payload_json(fields, |field| {
            if field.name != "currentAppInstallation" {
                return None;
            }
            let value = if self.store.staged.uninstalled_app_ids.contains(&app_id) {
                Value::Null
            } else {
                installation
                    .as_ref()
                    .map(|installation| {
                        current_app_installation_json(
                            installation,
                            &self.store.staged.app_subscriptions,
                            &self.store.staged.app_one_time_purchases,
                            &revoked_access_scopes,
                            &field.selection,
                        )
                    })
                    .unwrap_or(Value::Null)
            };
            Some(value)
        })
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

    pub(in crate::proxy) fn app_uninstall(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || "appUninstall".to_string());
        let app_selection = selected_child_selection(&payload_selection, "app").unwrap_or_default();
        let requested_id = resolved_object_field(&arguments, "input")
            .and_then(|input| resolved_string_field(&input, "id"));

        let current_app_id = self.ensure_current_app_installation(request);
        let target_app_id = requested_id
            .as_deref()
            .map(normalize_app_gid)
            .unwrap_or_else(|| current_app_id.clone());

        let (app, user_errors) = match self.app_installation_for_app(&target_app_id).cloned() {
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
                    request,
                    query,
                    variables,
                    "appUninstall",
                    vec![target_app_id.clone()],
                );
                (
                    installation.get("app").cloned().unwrap_or(Value::Null),
                    vec![],
                )
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
}

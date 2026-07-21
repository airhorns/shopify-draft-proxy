use super::*;

impl DraftProxy {
    pub(crate) fn delegate_access_token_create(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let scopes = input
            .get("delegateAccessScope")
            .or_else(|| input.get("accessScopes"))
            .map(resolved_string_list)
            .unwrap_or_default();
        let expires_in = match input.get("expiresIn") {
            Some(ResolvedValue::Int(value)) => *value,
            _ => 3600,
        };
        let mut user_errors = Vec::new();
        if scopes.is_empty() {
            user_errors.push(user_error(
                Value::Null,
                "The access scope can't be empty.",
                Some("EMPTY_ACCESS_SCOPE"),
            ));
        } else if expires_in <= 0 {
            user_errors.push(user_error(
                Value::Null,
                "The expires_in value must be greater than 0.",
                Some("NEGATIVE_EXPIRES_IN"),
            ));
        } else if delegate_expires_after_parent(
            invocation.request,
            expires_in,
            &self.next_product_timestamp(),
        ) {
            user_errors.push(user_error(
                Value::Null,
                "The delegate token can't expire after the parent token.",
                Some("EXPIRES_AFTER_PARENT"),
            ));
        }
        let app_id = self.ensure_current_app_installation(invocation.request);
        let granted_scopes = self
            .app_installation_for_app(&app_id)
            .as_ref()
            .map(app_access_scope_handles)
            .unwrap_or_default();
        let legacy_default_scope = |scope: &str| {
            self.app_installation_for_app(&app_id)
                .as_ref()
                .and_then(|installation| installation.get("__draftProxySource"))
                .and_then(Value::as_str)
                .is_some_and(|source| matches!(source, "default" | "observed-identity-only"))
                && matches!(
                    scope,
                    "read_products" | "write_products" | "read_markets" | "write_markets"
                )
        };
        if user_errors.is_empty() {
            if let Some(scope) = scopes
                .iter()
                .find(|scope| !granted_scopes.contains(*scope) && !legacy_default_scope(scope))
            {
                user_errors.push(user_error(
                    Value::Null,
                    &format!("The access scope is invalid: {scope}"),
                    Some("UNKNOWN_SCOPES"),
                ));
            }
        }

        if !user_errors.is_empty() {
            if user_errors.iter().any(|error| {
                error.get("code").and_then(Value::as_str) == Some("EXPIRES_AFTER_PARENT")
            }) {
                self.record_mutation_log_entry(
                    invocation.request,
                    invocation.query,
                    invocation.variables,
                    "delegateAccessTokenCreate",
                    vec![],
                );
                if let Some(entry) = self.log_entries.last_mut() {
                    set_log_status(entry, "failed");
                }
            }
            let shop = self.store.effective_shop();
            return ResolverOutcome::value(json!({
                "delegateAccessToken": Value::Null,
                "shop": shop,
                "userErrors": user_errors,
            }));
        }

        let token = format!(
            "shpat_delegate_proxy_{}",
            self.store.staged.delegate_access_tokens.len() + 1
        );
        let parent_access_token = request_access_token(invocation.request)
            .unwrap_or_else(|| "shpat_parent_default".to_string());
        let created_at = self.next_product_timestamp();
        let record = json!({
            "accessToken": token,
            "accessScopes": scopes,
            "createdAt": created_at,
            "expiresIn": expires_in,
            "parentAccessToken": parent_access_token,
            "apiClientId": app_id
        });
        self.store
            .staged
            .delegate_access_tokens
            .insert(token.clone(), record.clone());
        self.record_mutation_log_entry(
            invocation.request,
            invocation.query,
            invocation.variables,
            "delegateAccessTokenCreate",
            vec![token],
        );
        let shop = self.store.effective_shop();

        ResolverOutcome::value(json!({
            "delegateAccessToken": record,
            "shop": shop,
            "userErrors": [],
        }))
    }

    pub(crate) fn delegate_access_token_destroy(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let token = resolved_string_field(&arguments, "accessToken").unwrap_or_default();
        let caller_token = request_access_token(invocation.request).unwrap_or_default();
        let caller_api_client_id = request_api_client_id(invocation.request);

        let mut status = false;
        let mut user_errors = Vec::new();
        if !caller_token.is_empty()
            && caller_token == token
            && !token.starts_with("shpat_delegate_proxy_")
        {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Can only delete delegate tokens.",
                "CAN_ONLY_DELETE_DELEGATE_TOKENS",
            ));
        } else if caller_token.starts_with("shpat_delegate_proxy_") && caller_token != token {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Access denied.",
                "ACCESS_DENIED",
            ));
        } else if self
            .store
            .staged
            .uninstalled_app_ids
            .contains(&normalize_app_gid(&caller_api_client_id))
        {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Access token does not exist.",
                "ACCESS_TOKEN_NOT_FOUND",
            ));
        } else if let Some(record) = self.store.staged.delegate_access_tokens.get(&token) {
            let token_api_client_id = record
                .get("apiClientId")
                .and_then(Value::as_str)
                .unwrap_or("gid://shopify/App/local");
            if normalize_app_gid(token_api_client_id) != normalize_app_gid(&caller_api_client_id) {
                user_errors.push(delegate_access_token_destroy_user_error(
                    "Access denied.",
                    "ACCESS_DENIED",
                ));
            } else {
                self.store.staged.delegate_access_tokens.remove(&token);
                self.record_mutation_log_entry(
                    invocation.request,
                    invocation.query,
                    invocation.variables,
                    "delegateAccessTokenDestroy",
                    vec![token],
                );
                status = true;
            }
        } else {
            user_errors.push(delegate_access_token_destroy_user_error(
                "Access token does not exist.",
                "ACCESS_TOKEN_NOT_FOUND",
            ));
        }
        let shop = self.store.effective_shop();

        ResolverOutcome::value(json!({
            "status": status,
            "shop": shop,
            "userErrors": user_errors,
        }))
    }

    pub(crate) fn app_revoke_access_scopes(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let scopes = arguments
            .get("scopes")
            .map(resolved_string_list)
            .unwrap_or_default();

        let mut user_errors = Vec::new();
        let app_id = self.ensure_current_app_installation(invocation.request);
        let installation = self.app_installation_for_app(&app_id);
        let granted_scopes = installation
            .as_ref()
            .map(app_access_scope_handles)
            .unwrap_or_default();
        let required_scopes = installation
            .as_ref()
            .map(app_required_access_scope_handles)
            .unwrap_or_default();
        let legacy_default_scope = |scope: &str| {
            installation
                .as_ref()
                .and_then(|installation| installation.get("__draftProxySource"))
                .and_then(Value::as_str)
                .is_some_and(|source| matches!(source, "default" | "observed-identity-only"))
                && matches!(scope, "read_products" | "write_products")
        };

        if app_revoke_access_scopes_missing_source_app(invocation.request) {
            user_errors.push(user_error(
                ["id"],
                "No app found on the access token.",
                Some("MISSING_SOURCE_APP"),
            ));
        } else {
            let has_unknown_scope = scopes
                .iter()
                .any(|scope| !granted_scopes.contains(scope) && !legacy_default_scope(scope));
            if has_unknown_scope {
                user_errors.push(user_error(
                    ["scopes"],
                    "The requested list of scopes to revoke includes invalid handles.",
                    Some("UNKNOWN_SCOPES"),
                ));
            } else if scopes.iter().any(|scope| required_scopes.contains(scope)) {
                user_errors.push(user_error(
                    ["scopes"],
                    "Scopes that are declared as required cannot be revoked.",
                    Some("CANNOT_REVOKE_REQUIRED_SCOPES"),
                ));
            }
        }

        let revoked = if user_errors.is_empty() {
            for scope in &scopes {
                self.store
                    .staged
                    .revoked_app_access_scopes
                    .entry(app_id.clone())
                    .or_default()
                    .insert(scope.clone());
            }
            scopes
                .iter()
                .map(|scope| {
                    installation
                        .as_ref()
                        .map(|installation| app_access_scope_value(installation, scope))
                        .unwrap_or_else(|| access_scope_json(scope, None))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let revoked_payload = if user_errors.is_empty() {
            Some(revoked)
        } else if app_revoke_access_scopes_missing_source_app(invocation.request) {
            Some(Vec::new())
        } else {
            None
        };
        if user_errors.is_empty() {
            self.record_mutation_log_entry(
                invocation.request,
                invocation.query,
                invocation.variables,
                "appRevokeAccessScopes",
                scopes.clone(),
            );
        }

        ResolverOutcome::value(json!({
            "revoked": revoked_payload.map(Value::Array).unwrap_or(Value::Null),
            "userErrors": user_errors,
        }))
    }
}

fn delegate_expires_after_parent(request: &Request, expires_in: i64, created_at: &str) -> bool {
    let Some(parent_expires_at) =
        request_header(request, "x-shopify-draft-proxy-access-token-expires-at")
            .and_then(|value| parse_rfc3339_epoch_seconds(&value))
    else {
        return false;
    };
    let Some(created_at) = parse_rfc3339_epoch_seconds(created_at) else {
        return false;
    };
    created_at + expires_in > parent_expires_at
}

fn app_revoke_access_scopes_missing_source_app(request: &Request) -> bool {
    request_header(request, "x-shopify-draft-proxy-source-app-missing")
        .as_deref()
        .is_some_and(|value| matches!(value, "1" | "true" | "TRUE" | "True"))
}

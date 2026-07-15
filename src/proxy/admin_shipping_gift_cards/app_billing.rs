use crate::proxy::*;

mod delegate_access;
mod installation;
mod purchases;
mod subscriptions;

impl DraftProxy {
    pub(in crate::proxy) fn resolve_apps_graphql(
        &mut self,
        context: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            root_name,
            mode,
            ..
        } = context;
        match mode {
            LocalResolverMode::OverlayRead if root_name == "currentAppInstallation" => {
                let request_app_id = request_app_gid(request);
                if self
                    .store
                    .staged
                    .uninstalled_app_ids
                    .contains(&request_app_id)
                    || self
                        .current_app_installation_app_id_for_request(&request_app_id)
                        .is_some()
                    || !self.store.staged.app_subscriptions.is_empty()
                    || !self.store.staged.app_one_time_purchases.is_empty()
                    || self
                        .store
                        .staged
                        .revoked_app_access_scopes
                        .get(&request_app_id)
                        .is_some_and(|scopes| !scopes.is_empty())
                    || self.config.read_mode == ReadMode::Snapshot
                {
                    let fields = match self.root_fields_or_error(query, variables) {
                        Ok(fields) => fields,
                        Err(response) => return response,
                    };
                    ok_json(json!({
                        "data": self.current_app_installation_read_data(request, &fields)
                    }))
                } else {
                    let response = (self.upstream_transport)(request.clone());
                    if response.status < 400 {
                        self.observe_current_app_installation_response(request, &response);
                    }
                    response
                }
            }
            LocalResolverMode::StageLocally => match root_name {
                "appSubscriptionCreate" => self.app_subscription_create(query, variables, request),
                "appSubscriptionCancel" => self.app_subscription_cancel(query, variables, request),
                "appSubscriptionTrialExtend" => {
                    self.app_subscription_trial_extend(query, variables, request)
                }
                "appSubscriptionLineItemUpdate" => {
                    self.app_subscription_line_item_update(query, variables, request)
                }
                "appUsageRecordCreate" => self.app_usage_record_create(query, variables, request),
                "appPurchaseOneTimeCreate" => {
                    self.app_purchase_one_time_create(query, variables, request)
                }
                "appRevokeAccessScopes" => self.app_revoke_access_scopes(query, variables, request),
                "delegateAccessTokenCreate" => {
                    self.delegate_access_token_create(query, variables, request)
                }
                "delegateAccessTokenDestroy" => {
                    self.delegate_access_token_destroy(query, variables, request)
                }
                "appUninstall" => self.app_uninstall(query, variables, request),
                _ => Self::unimplemented_resolver_response(mode, root_name),
            },
            LocalResolverMode::OverlayRead => {
                Self::unimplemented_resolver_response(mode, root_name)
            }
        }
    }
}

fn app_domain_confirmation_url_from_arguments(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> String {
    resolved_string_field(arguments, "returnUrl")
        .filter(|value| !value.trim().is_empty())
        .map(|value| app_confirmation_url_with_marker(&value))
        .unwrap_or_else(|| {
            app_confirmation_url_with_marker("shopify-draft-proxy://local-confirmation")
        })
}

fn app_domain_confirmation_url_for_request(
    request: &Request,
    shopify_admin_origin: &str,
) -> String {
    let base = request_header(request, "x-shopify-draft-proxy-app-url")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| shopify_admin_origin.to_string());
    let base = app_local_confirmation_base_url(&base);
    app_confirmation_url_with_marker(&base)
}

fn app_local_confirmation_base_url(base: &str) -> String {
    match url::Url::parse(base) {
        Ok(mut url) if matches!(url.path(), "" | "/") => {
            url.set_path("/local-confirmation");
            url.to_string()
        }
        _ => base.to_string(),
    }
}

fn app_confirmation_url_with_marker(base: &str) -> String {
    match url::Url::parse(base) {
        Ok(mut url) => {
            url.query_pairs_mut()
                .append_pair("shopify_draft_proxy_confirmation", "1");
            url.to_string()
        }
        Err(_) => {
            let separator = if base.contains('?') { '&' } else { '?' };
            format!("{base}{separator}shopify_draft_proxy_confirmation=1")
        }
    }
}

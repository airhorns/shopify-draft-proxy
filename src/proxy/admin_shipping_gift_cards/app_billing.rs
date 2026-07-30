use crate::proxy::*;

mod delegate_access;
mod installation;
mod purchases;
mod subscriptions;

pub(in crate::proxy) fn app_billing_field_resolver_registrations() -> Vec<FieldResolverRegistration>
{
    [
        (
            "AppInstallation",
            "allSubscriptions",
            app_installation_all_subscriptions_field
                as crate::resolver_registry::FieldResolverHandler,
        ),
        (
            "AppInstallation",
            "oneTimePurchases",
            app_installation_one_time_purchases_field,
        ),
        (
            "AppSubscriptionLineItem",
            "usageRecords",
            app_subscription_line_item_usage_records_field,
        ),
        (
            "DelegateAccessTokenCreatePayload",
            "shop",
            mutation_payload_shop_field,
        ),
        (
            "DelegateAccessTokenDestroyPayload",
            "shop",
            mutation_payload_shop_field,
        ),
    ]
    .into_iter()
    .map(|(parent_type, field_name, handler)| {
        FieldResolverRegistration::explicit(ApiSurface::Admin, parent_type, field_name, handler)
    })
    .collect()
}

pub(in crate::proxy) fn app_billing_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    [
        "App",
        "AppInstallation",
        "AppPurchaseOneTime",
        "AppSubscription",
        "AppSubscriptionLineItem",
        "AppUsageRecord",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing app-billing field has no explicit canonical resolver",
        )
    })
    .collect()
}

fn app_installation_all_subscriptions_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let app_id = invocation.parent.pointer("/app/id").and_then(Value::as_str);
    let records = if proxy.store.staged.app_subscriptions.is_empty() {
        connection_nodes(&invocation.parent["allSubscriptions"])
    } else {
        proxy
            .store
            .staged
            .app_subscriptions
            .values()
            .filter(|subscription| {
                subscription
                    .get("__draftProxyAppId")
                    .and_then(Value::as_str)
                    .is_none_or(|owner| Some(owner) == app_id)
            })
            .cloned()
            .collect()
    };
    Ok(connection_value_with_args(
        records,
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn app_installation_one_time_purchases_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let app_id = invocation.parent.pointer("/app/id").and_then(Value::as_str);
    let records = if proxy.store.staged.app_one_time_purchases.is_empty() {
        connection_nodes(&invocation.parent["oneTimePurchases"])
    } else {
        proxy
            .store
            .staged
            .app_one_time_purchases
            .values()
            .filter(|purchase| {
                purchase
                    .get("__draftProxyAppId")
                    .and_then(Value::as_str)
                    .is_none_or(|owner| Some(owner) == app_id)
            })
            .cloned()
            .collect()
    };
    Ok(connection_value_with_args(
        records,
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

fn app_subscription_line_item_usage_records_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(connection_value_with_args(
        connection_nodes(&invocation.parent["usageRecords"]),
        &resolved_arguments_from_json(&invocation.arguments),
        value_id_cursor,
    ))
}

impl DraftProxy {
    pub(crate) fn current_app_installation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let request = invocation.request;
        let has_local_overlay = self.app_graph_has_local_overlay();
        if self.config.read_mode == ReadMode::LiveHybrid {
            let result =
                self.cached_or_forward_upstream_graphql_result(request, invocation.response_key);
            if result.transport_succeeded {
                self.observe_app_query_data(&invocation, &result.data);
            }
            if !has_local_overlay || !result.outcome.errors.is_empty() {
                return result.outcome;
            }
        }
        ResolverOutcome::value(self.current_app_installation_root_value(request))
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

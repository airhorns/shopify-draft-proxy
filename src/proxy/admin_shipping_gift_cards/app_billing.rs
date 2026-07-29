use crate::proxy::*;

mod delegate_access;
mod installation;
mod purchases;
mod subscriptions;

const APP_SUBSCRIPTION_OWNER_APP_ID_FIELD: &str = "__draftProxyOwnerAppId";
const APP_SUBSCRIPTION_MUTATION_HYDRATED_FIELD: &str = "__draftProxyMutationHydrated";
const APP_SUBSCRIPTION_LINE_ITEMS_HYDRATED_FIELD: &str = "__draftProxyLineItemsHydrated";
const APP_SUBSCRIPTION_HYDRATE_QUERY: &str = r#"query DraftProxyAppSubscriptionHydrate($id: ID!) {
  currentAppInstallation { id app { id } }
  node(id: $id) {
    ... on AppSubscription {
      id
      name
      status
      test
      trialDays
      currentPeriodEnd
      createdAt
      returnUrl
      lineItems {
        id
        plan {
          pricingDetails {
            __typename
            ... on AppRecurringPricing {
              price { amount currencyCode }
              interval
              planHandle
            }
            ... on AppUsagePricing {
              cappedAmount { amount currencyCode }
              balanceUsed { amount currencyCode }
              interval
              terms
            }
          }
        }
        usageRecords(first: 250) {
          nodes { id createdAt description idempotencyKey price { amount currencyCode } }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }
    }
  }
}"#;
const APP_SUBSCRIPTION_LINE_ITEM_HYDRATE_QUERY: &str = r#"query DraftProxyAppSubscriptionLineItemHydrate {
  currentAppInstallation {
    id
    app { id }
    activeSubscriptions {
      id
      name
      status
      test
      trialDays
      currentPeriodEnd
      createdAt
      returnUrl
      lineItems {
        id
        plan {
          pricingDetails {
            __typename
            ... on AppRecurringPricing {
              price { amount currencyCode }
              interval
              planHandle
            }
            ... on AppUsagePricing {
              cappedAmount { amount currencyCode }
              balanceUsed { amount currencyCode }
              interval
              terms
            }
          }
        }
        usageRecords(first: 250) {
          nodes { id createdAt description idempotencyKey price { amount currencyCode } }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }
    }
  }
}"#;

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
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let app_id = app_id_from_installation(invocation.parent)
        .unwrap_or_else(|| proxy.app_subscription_app_id_for_request(request));
    let records = proxy.effective_app_subscriptions_for_app(&app_id);
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
    let records = if proxy.store.staged.app_one_time_purchases.is_empty() {
        connection_nodes(&invocation.parent["oneTimePurchases"])
    } else {
        proxy
            .store
            .staged
            .app_one_time_purchases
            .values()
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
        let request_app_id = request_app_gid(request);
        if self
            .store
            .staged
            .uninstalled_app_ids
            .contains(&request_app_id)
        {
            return ResolverOutcome::value(self.current_app_installation_root_value(request));
        }

        if self.config.read_mode == ReadMode::LiveHybrid
            && !self.store.staged.app_subscriptions.is_empty()
        {
            let mut result =
                self.cached_or_forward_upstream_graphql_result(request, invocation.response_key);
            if result.transport_succeeded && result.outcome.errors.is_empty() {
                self.observe_current_app_installation_data(request, &result.data);
                result.outcome.value = self.current_app_installation_root_value(request);
                result.outcome.value_source = crate::admin_graphql::ResolverValueSource::Local;
            }
            return result.outcome;
        }

        if self
            .current_app_installation_app_id_for_request(&request_app_id)
            .is_some()
            || !self.store.staged.app_one_time_purchases.is_empty()
            || self
                .store
                .staged
                .revoked_app_access_scopes
                .get(&request_app_id)
                .is_some_and(|scopes| !scopes.is_empty())
            || self.config.read_mode == ReadMode::Snapshot
        {
            return ResolverOutcome::value(self.current_app_installation_root_value(request));
        }

        let result =
            self.cached_or_forward_upstream_graphql_result(request, invocation.response_key);
        if result.transport_succeeded {
            self.observe_current_app_installation_data(request, &result.data);
        }
        result.outcome
    }

    pub(in crate::proxy) fn app_subscription_app_id_for_request(
        &self,
        request: &Request,
    ) -> String {
        let request_app_id = request_app_gid(request);
        self.current_app_installation_app_id_for_request(&request_app_id)
            .unwrap_or(request_app_id)
    }

    pub(in crate::proxy) fn effective_app_subscriptions_for_app(&self, app_id: &str) -> Vec<Value> {
        effective_records(
            &self.store.base.app_subscriptions,
            &self.store.staged.app_subscriptions,
        )
        .into_iter()
        .filter(|subscription| app_subscription_belongs_to_app(subscription, app_id))
        .collect()
    }

    pub(in crate::proxy) fn effective_app_subscription_for_request(
        &self,
        request: &Request,
        id: &str,
    ) -> Option<Value> {
        let app_id = self.app_subscription_app_id_for_request(request);
        effective_get(
            &self.store.base.app_subscriptions,
            &self.store.staged.app_subscriptions,
            id,
        )
        .filter(|subscription| app_subscription_belongs_to_app(subscription, &app_id))
        .cloned()
    }

    pub(in crate::proxy) fn observe_base_app_subscription(
        &mut self,
        request: &Request,
        app_id: &str,
        observed: &Value,
    ) -> Option<Value> {
        let id = observed.get("id").and_then(Value::as_str)?.to_string();
        if shopify_gid_resource_type(&id) != Some("AppSubscription")
            || self.store.staged.app_subscriptions.is_tombstoned(&id)
        {
            return None;
        }
        let mut observed = observed.clone();
        annotate_observed_app_subscription(&mut observed, app_id, &request_api_client_id(request));
        let mut merged = self
            .store
            .base
            .app_subscriptions
            .get(&id)
            .cloned()
            .unwrap_or_else(|| json!({}));
        merge_app_billing_observation(&mut merged, &observed);
        self.store.base.app_subscriptions.insert(id, merged.clone());
        Some(merged)
    }

    pub(in crate::proxy) fn observe_app_subscriptions_from_installation(
        &mut self,
        request: &Request,
        app_id: &str,
        installation: &Value,
    ) {
        let mut subscriptions = connection_nodes(&installation["allSubscriptions"]);
        subscriptions.extend(
            installation
                .get("activeSubscriptions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|subscription| {
                    let id = subscription.get("id").and_then(Value::as_str);
                    !subscriptions
                        .iter()
                        .any(|existing| existing.get("id").and_then(Value::as_str) == id)
                })
                .cloned()
                .collect::<Vec<_>>(),
        );
        for subscription in subscriptions {
            self.observe_base_app_subscription(request, app_id, &subscription);
        }
    }

    pub(super) fn app_subscription_for_mutation(
        &mut self,
        request: &Request,
        id: &str,
    ) -> Option<Value> {
        if shopify_gid_resource_type(id) != Some("AppSubscription")
            || self.store.staged.app_subscriptions.is_tombstoned(id)
        {
            return None;
        }
        if self.store.staged.app_subscriptions.contains_key(id) {
            return self.effective_app_subscription_for_request(request, id);
        }
        let observed = self.effective_app_subscription_for_request(request, id);
        if self.store.base.app_subscriptions.get(id).is_some() && observed.is_none() {
            return None;
        }
        if observed.as_ref().is_some_and(|subscription| {
            subscription[APP_SUBSCRIPTION_MUTATION_HYDRATED_FIELD] == true
        }) {
            return observed;
        }
        self.hydrate_app_subscription_for_mutation(request, id)
            .or(observed)
    }

    fn hydrate_app_subscription_for_mutation(
        &mut self,
        request: &Request,
        id: &str,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": APP_SUBSCRIPTION_HYDRATE_QUERY,
                "operationName": "DraftProxyAppSubscriptionHydrate",
                "variables": { "id": id },
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return None;
        }
        self.observe_current_app_installation_data(request, &response.body["data"]);
        let app_id = self.app_subscription_app_id_for_request(request);
        let mut observed = response.body.pointer("/data/node")?.clone();
        observed[APP_SUBSCRIPTION_MUTATION_HYDRATED_FIELD] = json!(true);
        self.observe_base_app_subscription(request, &app_id, &observed)
    }

    pub(super) fn effective_app_subscription_line_item(
        &mut self,
        request: &Request,
        line_item_id: &str,
    ) -> Option<(String, usize, Value)> {
        if shopify_gid_resource_type(line_item_id) != Some("AppSubscriptionLineItem") {
            return None;
        }
        let observed = self.find_effective_app_subscription_line_item(request, line_item_id);
        if observed.as_ref().is_some_and(|(subscription_id, _, _)| {
            self.store
                .staged
                .app_subscriptions
                .contains_key(subscription_id)
        }) {
            return observed;
        }
        if observed.as_ref().is_some_and(|(_, _, subscription)| {
            subscription[APP_SUBSCRIPTION_LINE_ITEMS_HYDRATED_FIELD] == true
        }) {
            return observed;
        }
        self.hydrate_app_subscription_line_items_for_mutation(request);
        self.find_effective_app_subscription_line_item(request, line_item_id)
            .or(observed)
    }

    fn find_effective_app_subscription_line_item(
        &self,
        request: &Request,
        line_item_id: &str,
    ) -> Option<(String, usize, Value)> {
        let app_id = self.app_subscription_app_id_for_request(request);
        self.effective_app_subscriptions_for_app(&app_id)
            .into_iter()
            .find_map(|subscription| {
                let subscription_id = subscription["id"].as_str()?.to_string();
                let index = subscription["lineItems"]
                    .as_array()?
                    .iter()
                    .position(|line_item| line_item["id"] == line_item_id)?;
                Some((subscription_id, index, subscription))
            })
    }

    fn hydrate_app_subscription_line_items_for_mutation(&mut self, request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": APP_SUBSCRIPTION_LINE_ITEM_HYDRATE_QUERY,
                "operationName": "DraftProxyAppSubscriptionLineItemHydrate",
                "variables": {},
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return;
        }
        let mut data = response.body["data"].clone();
        if let Some(subscriptions) = data
            .pointer_mut("/currentAppInstallation/activeSubscriptions")
            .and_then(Value::as_array_mut)
        {
            for subscription in subscriptions {
                subscription[APP_SUBSCRIPTION_LINE_ITEMS_HYDRATED_FIELD] = json!(true);
            }
        }
        self.observe_current_app_installation_data(request, &data);
    }

    pub(super) fn stage_effective_app_subscription(
        &mut self,
        subscription_id: &str,
        subscription: Value,
    ) {
        self.store
            .staged
            .app_subscriptions
            .stage(subscription_id.to_string(), subscription);
    }
}

fn app_subscription_belongs_to_app(subscription: &Value, app_id: &str) -> bool {
    subscription
        .get(APP_SUBSCRIPTION_OWNER_APP_ID_FIELD)
        .and_then(Value::as_str)
        .is_none_or(|owner_app_id| owner_app_id == app_id)
}

fn annotate_observed_app_subscription(subscription: &mut Value, app_id: &str, api_client_id: &str) {
    subscription[APP_SUBSCRIPTION_OWNER_APP_ID_FIELD] = json!(app_id);
    let Some(line_items) = subscription
        .get_mut("lineItems")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for line_item in line_items {
        let line_item_shell = json!({ "id": line_item["id"].clone() });
        let Some(records) = line_item
            .pointer_mut("/usageRecords/nodes")
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for record in records {
            record["apiClientId"] = json!(api_client_id);
            record["subscriptionLineItem"] = line_item_shell.clone();
        }
    }
}

fn merge_app_billing_observation(existing: &mut Value, observed: &Value) {
    match (existing, observed) {
        (Value::Object(existing), Value::Object(observed)) => {
            for (key, observed_value) in observed {
                match existing.get_mut(key) {
                    Some(existing_value) => {
                        merge_app_billing_observation(existing_value, observed_value)
                    }
                    None => {
                        existing.insert(key.clone(), observed_value.clone());
                    }
                }
            }
        }
        (Value::Array(existing), Value::Array(observed))
            if observed
                .iter()
                .all(|value| value.get("id").and_then(Value::as_str).is_some()) =>
        {
            for observed_value in observed {
                let observed_id = observed_value["id"].as_str().unwrap_or_default();
                if let Some(existing_value) = existing
                    .iter_mut()
                    .find(|value| value.get("id").and_then(Value::as_str) == Some(observed_id))
                {
                    merge_app_billing_observation(existing_value, observed_value);
                } else {
                    existing.push(observed_value.clone());
                }
            }
        }
        (existing, observed) => *existing = observed.clone(),
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

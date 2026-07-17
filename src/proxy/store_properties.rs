use super::*;
use std::sync::OnceLock;

pub(in crate::proxy) fn store_property_field_resolver_registrations(
) -> Vec<FieldResolverRegistration> {
    vec![
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "ShopAddress",
            "formatted",
            shop_address_formatted_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "ShopPolicy",
            "translations",
            shop_policy_translations_field,
        ),
    ]
}

pub(in crate::proxy) fn store_property_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy>
{
    [
        "BusinessEntity",
        "Location",
        "LocationAddress",
        "LocationSuggestedAddress",
        "Shop",
        "ShopAddress",
        "ShopFeatures",
        "ShopPolicy",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing store-property field has no explicit canonical resolver",
        )
    })
    .collect()
}

fn shop_address_formatted_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(invocation
        .parent
        .get(&invocation.response_key)
        .or_else(|| invocation.parent.get("formatted"))
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new())))
}

fn shop_policy_translations_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let locale = resolved_string_field(&arguments, "locale").unwrap_or_default();
    let market_id = resolved_string_field(&arguments, "marketId");
    let translations = invocation
        .parent
        .get(&invocation.response_key)
        .or_else(|| invocation.parent.get("translations"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|translation| {
            locale.is_empty()
                || translation.get("locale").and_then(Value::as_str) == Some(locale.as_str())
        })
        .filter(|translation| {
            market_id.as_deref().is_none_or(|market_id| {
                translation.pointer("/market/id").and_then(Value::as_str) == Some(market_id)
                    || translation.get("marketId").and_then(Value::as_str) == Some(market_id)
            })
        })
        .cloned()
        .collect();
    Ok(Value::Array(translations))
}

const SHOP_POLICY_BODY_MAX_BYTES: usize = 524_287;
const SHOP_POLICY_TIMESTAMP: &str = "2024-01-01T00:00:00.000Z";
const SHOP_POLICY_TYPE_VALUES: &[&str] = &[
    "REFUND_POLICY",
    "SHIPPING_POLICY",
    "PRIVACY_POLICY",
    "TERMS_OF_SERVICE",
    "TERMS_OF_SALE",
    "LEGAL_NOTICE",
    "SUBSCRIPTION_POLICY",
    "CONTACT_INFORMATION",
];
// Must match the recorded `StorePropertiesShopBaselineHydrate` upstream call
// byte-for-byte (the strict cassette matcher compares the outgoing query against
// the recorded entry). A narrower shop-policy-only hydrate document does not
// match that cassette, causing shop-policy hydration to fail in parity runs and
// the proxy to fall back to synthetic policy ids/timestamps.
const SHOP_POLICY_HYDRATE_QUERY: &str = "query StorePropertiesShopBaselineHydrate { shop { id name myshopifyDomain url primaryDomain { id host url sslEnabled } contactEmail email currencyCode enabledPresentmentCurrencies ianaTimezone timezoneAbbreviation timezoneOffset timezoneOffsetMinutes taxesIncluded taxShipping unitSystem weightUnit shopAddress { id address1 address2 city company coordinatesValidated country countryCodeV2 formatted formattedArea latitude longitude phone province provinceCode zip } plan { partnerDevelopment publicDisplayName shopifyPlus } resourceLimits { locationLimit maxProductOptions maxProductVariants redirectLimitReached } features { avalaraAvatax branding bundles { eligibleForBundles ineligibilityReason sellsBundles } captcha cartTransform { eligibleOperations { expandOperation mergeOperation updateOperation } } dynamicRemarketing eligibleForSubscriptionMigration eligibleForSubscriptions giftCards harmonizedSystemCode legacySubscriptionGatewayEnabled liveView paypalExpressSubscriptionGatewayStatus reports sellsSubscriptions showMetrics storefront unifiedMarkets } paymentSettings { supportedDigitalWallets } shopPolicies { id title body type url createdAt updatedAt } } }";
const SHOP_PRICING_HYDRATE_QUERY: &str =
    "query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }";
// This document predates the shared shop-context loader and is already recorded
// byte-for-byte in product mutation cassettes. Reuse it for every payload that
// needs the same identity slice instead of proliferating domain-specific shop
// queries. The historical operation name is intentionally retained until those
// captures are refreshed together.
pub(in crate::proxy) const SHOP_IDENTITY_HYDRATE_QUERY: &str = r#"#graphql
  query ProductPayloadShopHydrate {
    shop {
      id
      name
      myshopifyDomain
      url
      currencyCode
      primaryDomain {
        id
        host
        url
        sslEnabled
      }
    }
  }
"#;

impl DraftProxy {
    pub(crate) fn store_properties_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let requests_owner_metafields = invocation.requested_field_paths.iter().any(|path| {
            path.first()
                .is_some_and(|field| matches!(field.as_str(), "metafield" | "metafields"))
        });
        let RootInvocation {
            response_key,
            request,
            root_name,
            arguments,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        match root_name {
            "shop" => {
                if requests_owner_metafields || self.should_handle_shop_policy_query_locally() {
                    return ResolverOutcome::value(
                        self.shop_canonical_value(&self.store.effective_shop()),
                    );
                }
                let result = self.cached_or_forward_upstream_graphql_result(request, response_key);
                if result.transport_succeeded {
                    self.hydrate_shop_state_from_response_data(&result.data);
                    self.observe_nodes_data(&json!({ "data": result.data.clone() }));
                }
                result.outcome
            }
            _ if self.has_location_overlay_state()
                || !self.location_root_needs_upstream(root_name, &arguments) =>
            {
                self.location_root_outcome(root_name, &arguments, response_key)
            }
            _ => self.cached_or_forward_upstream_root_outcome(request, response_key),
        }
    }

    pub(in crate::proxy) fn shop_has_observed_identity(&self) -> bool {
        self.store
            .base
            .shop
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| {
                let id = id.trim();
                !id.is_empty() && !id.contains("shopify-draft-proxy=synthetic")
            })
    }

    /// Hydrate the canonical Shop value for a mutation payload. Native field
    /// resolvers call this only when the client actually selects `shop`, so the
    /// domain mutation itself never needs to inspect its GraphQL selection.
    pub(in crate::proxy) fn hydrate_payload_shop_identity(&mut self, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot || self.shop_has_observed_identity() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": SHOP_IDENTITY_HYDRATE_QUERY,
                "variables": {}
            }),
        );
        if (200..300).contains(&response.status) && response.body.get("errors").is_none() {
            self.hydrate_shop_state_from_response_data(&response.body["data"]);
        }
    }

    pub(in crate::proxy) fn hydrate_shop_pricing_state_if_missing(
        &mut self,
        request: &Request,
        needs_currency: bool,
        needs_tax_flags: bool,
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let missing_currency = needs_currency && self.store.observed_shop_currency_code().is_none();
        let missing_tax_flags = needs_tax_flags && self.store.shop_taxes_included().is_none();
        if !missing_currency && !missing_tax_flags {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": SHOP_PRICING_HYDRATE_QUERY,
                "variables": {}
            }),
        );
        if let Some(shop) = response.body["data"]
            .get("shop")
            .filter(|shop| shop.is_object())
        {
            self.store.base.shop =
                shallow_merged_object(self.store.base.shop.clone(), shop.clone());
        }
    }

    pub(crate) fn shop_policy_update_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if let Some(error) = self.shop_policy_update_invalid_variable_error(
            invocation.query,
            &invocation.raw_arguments,
            invocation.root_location,
        ) {
            return ResolverOutcome::value(Value::Null).with_errors(root_field_errors_from_json(
                &[error],
                invocation.response_key,
            ));
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let payload = self.shop_policy_update_payload(invocation.request, &arguments);
        let staged_id = payload
            .pointer("/shopPolicy/id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(id) = staged_id {
            ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
                "shopPolicyUpdate",
                "store-properties",
                vec![id],
            ))
        } else {
            ResolverOutcome::value(payload)
        }
    }

    fn shop_policy_update_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let input = match arguments.get("shopPolicy") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return json!({
                    "shopPolicy": Value::Null,
                    "userErrors": [user_error_with_code_value(vec!["shopPolicy"], "Shop policy is invalid", json!("INVALID"))]
                });
            }
        };
        let policy_type = resolved_string_field(input, "type").unwrap_or_default();
        let body = resolved_string_field(input, "body").unwrap_or_default();
        if policy_type == "SUBSCRIPTION_POLICY" && body.trim().is_empty() {
            return json!({
                "shopPolicy": Value::Null,
                "userErrors": [user_error_with_code_value(vec!["shopPolicy", "body"], "Purchase options cancellation policy required", Value::Null)]
            });
        }
        if body.len() > SHOP_POLICY_BODY_MAX_BYTES {
            return json!({
                "shopPolicy": Value::Null,
                "userErrors": [user_error_with_code_value(vec!["shopPolicy", "body"], "Body is too big (maximum is 512 KB)", json!("TOO_BIG"))]
            });
        }
        if policy_type == "PRIVACY_POLICY" {
            if let Some(message) = shop_policy_liquid_syntax_error_message(&body) {
                return json!({
                    "shopPolicy": Value::Null,
                    "userErrors": [user_error_with_code_value(
                        vec!["shopPolicy", "body"],
                        &message,
                        Value::Null
                    )]
                });
            }
        }

        self.hydrate_shop_policy_base(request);
        let existing = self.store.shop_policy_by_type(&policy_type).cloned();
        let id = existing
            .as_ref()
            .map(|policy| policy.id.clone())
            .unwrap_or_else(|| self.next_shop_policy_id());
        let created_at = existing
            .as_ref()
            .map(|policy| policy.created_at.clone())
            .unwrap_or_else(|| SHOP_POLICY_TIMESTAMP.to_string());
        let url = self.shop_policy_url_for_id(&id);
        let policy = ShopPolicyRecord {
            id,
            policy_type: policy_type.clone(),
            title: shop_policy_title(&policy_type)
                .unwrap_or(&policy_type)
                .to_string(),
            body,
            url,
            created_at,
            updated_at: SHOP_POLICY_TIMESTAMP.to_string(),
            translations: existing
                .map(|policy| policy.translations)
                .unwrap_or_default(),
        };
        self.store.stage_shop_policy(policy.clone());

        json!({
            "shopPolicy": shop_policy_record_json(&policy),
            "userErrors": []
        })
    }

    fn shop_policy_update_invalid_variable_error(
        &self,
        query: &str,
        raw_arguments: &BTreeMap<String, RawArgumentValue>,
        root_location: SourceLocation,
    ) -> Option<Value> {
        let RawArgumentValue::Variable { name, value } = raw_arguments.get("shopPolicy")? else {
            return None;
        };
        let input = match value {
            Some(ResolvedValue::Object(input)) => input,
            _ => return None,
        };
        shop_policy_update_invalid_input_response(query, name, input, root_location)
    }

    fn next_shop_policy_id(&mut self) -> String {
        let id = synthetic_shopify_gid("ShopPolicy", self.next_synthetic_id);
        self.next_synthetic_id += 1;
        id
    }

    fn shop_policy_url_for_id(&self, id: &str) -> String {
        let policy_path_id = resource_id_tail(id);
        let shop = self.store.effective_shop();
        if let Some(shop_path_id) = shop_policy_checkout_shop_id(&shop) {
            return format!(
                "https://checkout.shopify.com/{shop_path_id}/policies/{policy_path_id}.html?locale=en"
            );
        }
        let domain = effective_shop_domain(&shop);
        format!("https://{domain}/policies/{policy_path_id}.html?locale=en")
    }

    pub(in crate::proxy) fn hydrate_shop_policy_base(&mut self, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot
            || !self.store.shop_policies.base.records.is_empty()
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": SHOP_POLICY_HYDRATE_QUERY,
                "variables": {}
            }),
        );
        if (200..300).contains(&response.status) {
            // Populate the structured shop-policy records (read by
            // `shop_policy_by_type`/`shop_policies`); `hydrate_shop_state_from_response_data`
            // only stores the raw `base.shop` blob.
            if let Some(shop) = response.body["data"]
                .get("shop")
                .filter(|shop| shop.is_object())
            {
                let (policies, order) = shop_policy_state_from_shop(shop);
                self.store
                    .shop_policies
                    .base
                    .replace_with_order(policies, order);
            }
            self.hydrate_shop_state_from_response_data(&response.body["data"]);
        }
    }

    pub(in crate::proxy) fn should_handle_shop_policy_query_locally(&self) -> bool {
        self.config.read_mode == ReadMode::Snapshot || self.store.shop_policies.has_state()
    }

    pub(in crate::proxy) fn shop_canonical_value(&self, shop: &Value) -> Value {
        let mut shop = shop.clone();
        if shop.get("id").and_then(Value::as_str).is_none() {
            if let Some(owner_id) = self
                .store
                .staged
                .owner_metafields
                .keys()
                .find(|id| shopify_gid_resource_type(id) == Some("Shop"))
            {
                shop["id"] = json!(owner_id);
            }
        }
        shop["__typename"] = json!("Shop");
        shop["shopPolicies"] = Value::Array(
            self.store
                .shop_policies()
                .into_iter()
                .map(|policy| shop_policy_record_json(&policy))
                .collect(),
        );
        shop
    }

    pub(in crate::proxy) fn shop_property_node_value_by_id(&self, id: &str) -> Option<Value> {
        match shopify_gid_resource_type(id)? {
            "ShopAddress" => self.shop_address_node_value(id),
            "ShopPolicy" => self
                .store
                .shop_policy_by_id(id)
                .map(shop_policy_record_json),
            _ => None,
        }
    }

    fn shop_address_node_value(&self, id: &str) -> Option<Value> {
        let shop = self.store.effective_shop();
        let address = shop.get("shopAddress")?;
        if address.get("id").and_then(Value::as_str) != Some(id) {
            return None;
        }
        Some(address.clone())
    }

    pub(in crate::proxy) fn observe_shop_property_node(&mut self, node: &Value) {
        let Some(id) = node.get("id").and_then(Value::as_str) else {
            return;
        };
        match shopify_gid_resource_type(id) {
            Some("ShopAddress") => {
                let mut address = node.clone();
                if let Some(object) = address.as_object_mut() {
                    object.remove("__typename");
                }
                if !self.store.base.shop.is_object() {
                    self.store.base.shop = json!({});
                }
                self.store.base.shop["shopAddress"] = address;
            }
            Some("ShopPolicy") => {
                if let Some(policy) = shop_policy_record_from_json(node) {
                    self.store
                        .shop_policies
                        .base
                        .insert(policy.id.clone(), policy);
                }
            }
            _ => {}
        }
    }
}

pub(in crate::proxy) fn mutation_payload_shop_field(
    proxy: &mut DraftProxy,
    request: &Request,
    _invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    proxy.hydrate_payload_shop_identity(request);
    Ok(proxy.shop_canonical_value(&proxy.store.effective_shop()))
}

pub(in crate::proxy) fn shop_policy_record_json(policy: &ShopPolicyRecord) -> Value {
    json!({
        "__typename": "ShopPolicy",
        "id": policy.id,
        "title": policy.title,
        "body": policy.body,
        "type": policy.policy_type,
        "url": policy.url,
        "createdAt": policy.created_at,
        "updatedAt": policy.updated_at,
        "translations": policy.translations
    })
}

pub(in crate::proxy) fn shop_policy_state_map_json(
    policies: &BTreeMap<String, ShopPolicyRecord>,
) -> serde_json::Map<String, Value> {
    policies
        .iter()
        .map(|(id, policy)| (id.clone(), shop_policy_record_json(policy)))
        .collect()
}

pub(in crate::proxy) fn shop_policy_state_map_from_json(
    value: &Value,
) -> BTreeMap<String, ShopPolicyRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, policy)| {
            shop_policy_record_from_json(policy).map(|policy| (id.clone(), policy))
        })
        .collect()
}

pub(in crate::proxy) fn shop_policy_state_from_shop(
    shop: &Value,
) -> (BTreeMap<String, ShopPolicyRecord>, Vec<String>) {
    let policies = shop
        .get("shopPolicies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(shop_policy_record_from_json)
        .collect::<Vec<_>>();
    let order = policies
        .iter()
        .map(|policy| policy.id.clone())
        .collect::<Vec<_>>();
    let records = policies
        .into_iter()
        .map(|policy| (policy.id.clone(), policy))
        .collect::<BTreeMap<_, _>>();
    (records, order)
}

fn shop_policy_record_from_json(value: &Value) -> Option<ShopPolicyRecord> {
    let id = value.get("id")?.as_str()?.to_string();
    let policy_type = value.get("type")?.as_str()?.to_string();
    Some(ShopPolicyRecord {
        id,
        policy_type,
        title: value.get("title")?.as_str()?.to_string(),
        body: value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        url: value
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        created_at: value
            .get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or(SHOP_POLICY_TIMESTAMP)
            .to_string(),
        updated_at: value
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or(SHOP_POLICY_TIMESTAMP)
            .to_string(),
        translations: value
            .get("translations")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    })
}

fn shop_policy_update_invalid_input_response(
    query: &str,
    name: &str,
    input: &BTreeMap<String, ResolvedValue>,
    field_location: SourceLocation,
) -> Option<Value> {
    let variable = variable_definition_info(query, name);
    let variable_name = name;
    let variable_type = variable
        .as_ref()
        .map(|definition| definition.type_display.as_str())
        .unwrap_or("ShopPolicyInput!");
    let location = variable
        .as_ref()
        .map(|definition| definition.location)
        .unwrap_or(field_location);
    let mut problems = Vec::new();
    match input.get("type") {
        Some(ResolvedValue::String(policy_type))
            if SHOP_POLICY_TYPE_VALUES.contains(&policy_type.as_str()) => {}
        Some(ResolvedValue::String(policy_type)) => {
            problems.push(json!({
                "path": ["type"],
                "explanation": format!(
                    "Expected \"{policy_type}\" to be one of: {}",
                    SHOP_POLICY_TYPE_VALUES.join(", ")
                )
            }));
        }
        Some(ResolvedValue::Null) | None => {
            problems.push(json!({
                "path": ["type"],
                "explanation": "Expected value to not be null"
            }));
        }
        _ => {
            problems.push(json!({
                "path": ["type"],
                "explanation": "Expected value to be a string"
            }));
        }
    }
    if !matches!(input.get("body"), Some(ResolvedValue::String(_))) {
        problems.push(json!({
            "path": ["body"],
            "explanation": "Expected value to not be null"
        }));
    }
    if problems.is_empty() {
        return None;
    }
    Some(invalid_variable_error(
        VariableValidationContext {
            variable_name,
            variable_type,
            location,
        },
        &ResolvedValue::Object(input.clone()),
        problems,
    ))
}

fn shop_policy_liquid_syntax_error_message(body: &str) -> Option<String> {
    static PARSER: OnceLock<liquid::Parser> = OnceLock::new();
    let parser = PARSER.get_or_init(|| {
        liquid::ParserBuilder::with_stdlib()
            .build()
            .expect("shop policy Liquid parser should build")
    });
    parser.parse(body).err().map(|error| {
        format!(
            "Body Liquid syntax error: {}",
            shop_policy_liquid_error_detail(&error.to_string())
        )
    })
}

fn shop_policy_liquid_error_detail(error: &str) -> String {
    if let Some(tag) = error.lines().find_map(|line| {
        line.trim()
            .strip_prefix("requested=")
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
    }) {
        return format!("Unknown tag '{tag}'");
    }

    if let Some(message) = error.lines().find_map(|line| {
        line.trim()
            .strip_prefix("= ")
            .map(str::trim)
            .filter(|message| !message.is_empty())
    }) {
        return message.trim_end_matches('.').to_string();
    }

    error
        .trim()
        .strip_prefix("liquid:")
        .unwrap_or(error.trim())
        .lines()
        .next()
        .unwrap_or("Invalid Liquid syntax")
        .trim()
        .trim_end_matches('.')
        .to_string()
}

fn shop_policy_title(policy_type: &str) -> Option<&'static str> {
    Some(match policy_type {
        "PRIVACY_POLICY" => "Privacy Policy",
        "REFUND_POLICY" => "Refund Policy",
        "TERMS_OF_SERVICE" => "Terms of Service",
        "SHIPPING_POLICY" => "Shipping Policy",
        "SUBSCRIPTION_POLICY" => "Subscription Policy",
        "CONTACT_INFORMATION" => "Contact Information",
        "LEGAL_NOTICE" => "Legal Notice",
        "TERMS_OF_SALE" => "Terms of Sale",
        _ => return None,
    })
}

fn effective_shop_domain(shop: &Value) -> String {
    shop.get("primaryDomain")
        .and_then(|primary_domain| primary_domain.get("host"))
        .and_then(Value::as_str)
        .or_else(|| shop.get("myshopifyDomain").and_then(Value::as_str))
        .or_else(|| {
            shop.get("url")
                .and_then(Value::as_str)
                .and_then(|url| {
                    url.strip_prefix("https://")
                        .or_else(|| url.strip_prefix("http://"))
                })
                .map(|host| host.trim_end_matches('/'))
        })
        .unwrap_or("shopify-draft-proxy.local")
        .to_string()
}

fn shop_policy_checkout_shop_id(shop: &Value) -> Option<&str> {
    let tail = shop
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| shopify_gid_tail_for_type(id, "Shop"))?;
    tail.chars().all(|c| c.is_ascii_digit()).then_some(tail)
}

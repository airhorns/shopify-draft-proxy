use super::*;
use std::sync::OnceLock;

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

impl DraftProxy {
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

    pub(in crate::proxy) fn shop_policy_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        if let Some(response) = shop_policy_update_invalid_variable_response(query, variables) {
            return response;
        }
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut staged_ids = Vec::new();
        let data = root_payload_json(&fields, |field| {
            if field.name != "shopPolicyUpdate" {
                return None;
            }
            let payload = self.shop_policy_update_field_payload(request, field);
            if let Some(id) = payload
                .get("shopPolicy")
                .and_then(Value::as_object)
                .and_then(|policy| policy.get("id"))
                .and_then(Value::as_str)
            {
                staged_ids.push(id.to_string());
            }
            Some(payload)
        });
        if !staged_ids.is_empty() {
            self.record_mutation_log_draft(
                request,
                query,
                variables,
                LogDraft::staged("shopPolicyUpdate", "store-properties", staged_ids),
            );
        }
        ok_json(json!({ "data": data }))
    }

    fn shop_policy_update_field_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> Value {
        let payload_selection = &field.selection;
        let input = match field.arguments.get("shopPolicy") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return selected_json(
                    &json!({
                        "shopPolicy": Value::Null,
                        "userErrors": [user_error_with_code_value(vec!["shopPolicy"], "Shop policy is invalid", json!("INVALID"))]
                    }),
                    payload_selection,
                );
            }
        };
        let policy_type = resolved_string_field(input, "type").unwrap_or_default();
        let body = resolved_string_field(input, "body").unwrap_or_default();
        if policy_type == "SUBSCRIPTION_POLICY" && body.trim().is_empty() {
            return selected_json(
                &json!({
                    "shopPolicy": Value::Null,
                    "userErrors": [user_error_with_code_value(vec!["shopPolicy", "body"], "Purchase options cancellation policy required", Value::Null)]
                }),
                payload_selection,
            );
        }
        if body.len() > SHOP_POLICY_BODY_MAX_BYTES {
            return selected_json(
                &json!({
                    "shopPolicy": Value::Null,
                    "userErrors": [user_error_with_code_value(vec!["shopPolicy", "body"], "Body is too big (maximum is 512 KB)", json!("TOO_BIG"))]
                }),
                payload_selection,
            );
        }
        if policy_type == "PRIVACY_POLICY" {
            if let Some(message) = shop_policy_liquid_syntax_error_message(&body) {
                return selected_json(
                    &json!({
                        "shopPolicy": Value::Null,
                        "userErrors": [user_error_with_code_value(
                            vec!["shopPolicy", "body"],
                            &message,
                            Value::Null
                        )]
                    }),
                    payload_selection,
                );
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

        selected_json(
            &json!({
                "shopPolicy": shop_policy_record_json(&policy),
                "userErrors": []
            }),
            payload_selection,
        )
    }

    fn next_shop_policy_id(&mut self) -> String {
        let id = synthetic_shopify_gid("ShopPolicy", self.next_synthetic_id);
        self.next_synthetic_id += 1;
        id
    }

    fn shop_policy_url_for_id(&self, id: &str) -> String {
        let domain = effective_shop_domain(&self.store.effective_shop());
        let policy_path_id = resource_id_tail(id);
        format!("https://{domain}/policies/{policy_path_id}.html?locale=en")
    }

    pub(in crate::proxy) fn hydrate_shop_policy_base(&mut self, request: &Request) {
        if self.config.read_mode == ReadMode::Snapshot
            || !self.store.base.shop_policies.records.is_empty()
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
                    .base
                    .shop_policies
                    .replace_with_order(policies, order);
            }
            self.hydrate_shop_state_from_response_data(&response.body["data"]);
        }
    }

    pub(in crate::proxy) fn shop_query_data(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let fields = root_fields(query, variables)?;
        if !fields
            .iter()
            .all(|field| matches!(field.name.as_str(), "shop" | "node" | "nodes"))
        {
            return None;
        }
        let mut data = serde_json::Map::new();
        let shop = self.store.effective_shop();
        for field in &fields {
            if field.name == "shop" {
                data.insert(
                    field.response_key.clone(),
                    self.shop_json(&shop, &field.selection),
                );
            }
        }
        if let Some(node_data) = self.shop_policy_node_read_data(&fields) {
            if let Some(node_data) = node_data.as_object() {
                data.extend(node_data.clone());
            }
        }
        Some(Value::Object(data))
    }

    pub(in crate::proxy) fn should_handle_shop_policy_query_locally(&self) -> bool {
        self.config.read_mode == ReadMode::Snapshot
            || !self.store.base.shop_policies.records.is_empty()
            || !self.store.staged.shop_policies.records.is_empty()
            || !self.store.staged.shop_policies.tombstones.is_empty()
    }

    fn shop_json(&self, shop: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |selection| match selection.name.as_str() {
            "shopPolicies" => Some(Value::Array(
                self.store
                    .shop_policies()
                    .into_iter()
                    .map(|policy| shop_policy_json(&policy, &selection.selection))
                    .collect(),
            )),
            _ => shop
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    pub(in crate::proxy) fn shop_policy_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        let mut saw_shop_policy = false;
        for field in fields {
            match field.name.as_str() {
                "node" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    if shopify_gid_resource_type(&id) == Some("ShopPolicy") {
                        saw_shop_policy = true;
                        data.insert(
                            field.response_key.clone(),
                            self.shop_policy_node_value(&id, &field.selection)
                                .unwrap_or(Value::Null),
                        );
                    } else {
                        data.insert(
                            field.response_key.clone(),
                            local_node_value(
                                &id,
                                &field.selection,
                                Some(&self.store.staged.backup_region),
                            )
                            .unwrap_or(Value::Null),
                        );
                    }
                }
                "nodes" => {
                    let ids = resolved_string_list_arg(&field.arguments, "ids");
                    if ids
                        .iter()
                        .any(|id| shopify_gid_resource_type(id) == Some("ShopPolicy"))
                    {
                        saw_shop_policy = true;
                        data.insert(
                            field.response_key.clone(),
                            Value::Array(
                                ids.into_iter()
                                    .map(|id| {
                                        if shopify_gid_resource_type(&id) == Some("ShopPolicy") {
                                            self.shop_policy_node_value(&id, &field.selection)
                                                .unwrap_or(Value::Null)
                                        } else {
                                            local_node_value(
                                                &id,
                                                &field.selection,
                                                Some(&self.store.staged.backup_region),
                                            )
                                            .unwrap_or(Value::Null)
                                        }
                                    })
                                    .collect(),
                            ),
                        );
                    }
                }
                _ => {}
            }
        }
        saw_shop_policy.then_some(Value::Object(data))
    }

    fn shop_policy_node_value(&self, id: &str, selection: &[SelectedField]) -> Option<Value> {
        self.store
            .shop_policy_by_id(id)
            .map(|policy| shop_policy_json(policy, selection))
            .or_else(|| local_node_value(id, selection, Some(&self.store.staged.backup_region)))
    }
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

fn shop_policy_json(policy: &ShopPolicyRecord, selection: &[SelectedField]) -> Value {
    selected_json(&shop_policy_record_json(policy), selection)
}

fn shop_policy_update_invalid_variable_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let field = root_fields(query, variables)?
        .into_iter()
        .find(|field| field.name == "shopPolicyUpdate")?;
    let RawArgumentValue::Variable { name, value } = field.raw_arguments.get("shopPolicy")? else {
        return None;
    };
    let input = match value {
        Some(ResolvedValue::Object(input)) => input,
        _ => return None,
    };
    let variable = variable_definition_info(query, name);
    let variable_name = name.as_str();
    let variable_type = variable
        .as_ref()
        .map(|definition| definition.type_display.as_str())
        .unwrap_or("ShopPolicyInput!");
    let location = variable
        .as_ref()
        .map(|definition| definition.location)
        .unwrap_or(field.location);
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
    Some(ok_json(json!({
        "errors": [invalid_variable_error(
            VariableValidationContext {
                variable_name,
                variable_type,
                location,
            },
            &ResolvedValue::Object(input.clone()),
            problems,
        )]
    })))
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
